import init, {
  create_swarm_cpu,
  create_swarm_gpu,
  mesh_cube,
  initThreadPool,
  artisan_rayon_threads,
} from "./pkg/murmuration.js";
import { ArtisanApp } from "../../packages/artisan-js/App.js";
import { WebGPURenderer } from "../../packages/artisan-js/Renderer.js";

const CUBE_SCALE = 0.11;
const BOUNDS = 34.0;
const SHADER_TYPE = 7.0;

const FlowComputeWGSL = `
  struct Particle {
      pos_x: f32, pos_y: f32, pos_z: f32,
      vel_x: f32, vel_y: f32, vel_z: f32,
      scale: f32,
      agility: f32,
  };
  @group(0) @binding(0) var<storage, read_write> particles: array<Particle>;

  struct SimParams {
      speed: f32, size: f32, gravity: f32, noise_scale: f32,
      time: f32, dt: f32, row_stride: f32, pad2: f32,
  };
  @group(0) @binding(1) var<uniform> params: SimParams;

  const BOUNDS: f32 = ${BOUNDS};
  const FLOW_SPEED: f32 = 7.0;

  fn hash_u32(x_in: u32) -> u32 {
      var a = x_in;
      a = (a ^ 61u) ^ (a >> 16u);
      a = a + (a << 3u);
      a = a ^ (a >> 4u);
      a = a * 0x27d4eb2du;
      a = a ^ (a >> 15u);
      return a;
  }
  fn hash_f32(x_in: u32) -> f32 {
      return f32(hash_u32(x_in)) / 4294967295.0;
  }

  fn start_pos(index: u32) -> vec3<f32> {
      let theta = hash_f32(index * 3u + 1u) * 6.2831853;
      let cos_phi = hash_f32(index * 7u + 13u) * 2.0 - 1.0;
      let sin_phi = sqrt(max(1.0 - cos_phi * cos_phi, 0.0));
      let dist = pow(hash_f32(index * 11u + 29u), 0.55) * BOUNDS * 0.75;
      return vec3<f32>(
          sin_phi * cos(theta) * dist,
          cos_phi * dist * 0.65,
          sin_phi * sin(theta) * dist
      );
  }

  fn flow_field(p: vec3<f32>, t: f32) -> vec3<f32> {
      let S: f32 = 0.09;
      var v = vec3<f32>(
          sin(p.y * S + t * 0.31) + cos(p.z * S * 1.3 - t * 0.21),
          sin(p.z * S * 1.1 + t * 0.27) + cos(p.x * S * 0.9 + t * 0.19),
          sin(p.x * S * 1.2 - t * 0.23) + cos(p.y * S * 1.05 + t * 0.25)
      );

      v.x += -p.z * 0.05;
      v.z +=  p.x * 0.05;

      let r = length(p);
      let over = max(r / BOUNDS - 0.6, 0.0);
      if (over > 0.0 && r > 0.001) {
          v -= (p / r) * (over * over * 3.0);
      }
      return v;
  }

  @compute @workgroup_size(64)
  fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {

      let index = id.x + id.y * u32(params.row_stride);
      if (index >= arrayLength(&particles)) { return; }
      var p = particles[index];

      if (abs(p.scale - 1.0) < 1e-6) {
          let sp = start_pos(index);
          p.pos_x = sp.x; p.pos_y = sp.y; p.pos_z = sp.z;
          p.vel_x = 0.0;  p.vel_y = 0.0;  p.vel_z = 0.0;
          p.agility = 0.7 + hash_f32(index * 17u + 71u) * 0.6;
      }

      let pos = vec3<f32>(p.pos_x, p.pos_y, p.pos_z);

      let desired = flow_field(pos, params.time) * (FLOW_SPEED * p.agility);

      let dt = min(params.dt, 0.05);
      let blend = 1.0 - exp(-dt * 2.5);
      var vel = vec3<f32>(p.vel_x, p.vel_y, p.vel_z);
      vel += (desired - vel) * blend;

      let new_pos = pos + vel * dt;
      p.pos_x = new_pos.x; p.pos_y = new_pos.y; p.pos_z = new_pos.z;
      p.vel_x = vel.x;     p.vel_y = vel.y;     p.vel_z = vel.z;
      p.scale = params.size;

      particles[index] = p;
  }
`;

const VS_GEOMETRY = {
  6: `
      let r_pos  = rotate_vector(vertex_pos, q);
      let r_norm = rotate_vector(vertex_normal, q);
`,
  3: `

      let q_conj = vec4<f32>(-q.xyz, q.w);
      let to_cam_local = rotate_vector(scene.camera_pos.xyz - inst_pos, q_conj);

      let slot   = vert_idx / 4u;
      let corner = vert_idx % 4u;

      let face_n = axis_vec(slot);
      let sgn = select(-1.0, 1.0, dot(to_cam_local, face_n) >= 0.0);

      let n = face_n * sgn;
      let u = axis_vec((slot + 1u) % 3u) * sgn;
      let v = axis_vec((slot + 2u) % 3u);
      let cu = select(-1.0, 1.0, corner == 1u || corner == 2u);
      let cv = select(-1.0, 1.0, corner == 2u || corner == 3u);

      let r_pos  = rotate_vector((u * cu + v * cv + n) * 0.5, q);
      let r_norm = rotate_vector(n, q);
`,
};

const makeFlowRenderWGSL = (faces) => `

  struct Scene {
      view_proj:      mat4x4<f32>,
      inv_view_proj:  mat4x4<f32>,
      camera_pos:     vec4<f32>,
      ambient:        vec4<f32>,
      ambient_ground: vec4<f32>,
      ambient_solid:  vec4<f32>,
      exposure:       vec4<f32>,
      dir0_dir:       vec4<f32>,
      dir0_color:     vec4<f32>,
  };
  @group(0) @binding(0) var<uniform> scene: Scene;

  struct VertexOutput {
      @builtin(position) position: vec4<f32>,
      @location(0) color: vec4<f32>,
      @location(1) normal: vec3<f32>,
      @location(2) emissive: vec3<f32>,
  };

  const BOUNDS: f32 = ${BOUNDS};

  const EMISSIVE: f32 = 0.3;

  fn srgbToLinear(c: vec3<f32>) -> vec3<f32> {
      return select(
          pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
          c / 12.92,
          c <= vec3<f32>(0.04045)
      );
  }
  fn linearToSrgb(c: vec3<f32>) -> vec3<f32> {
      return select(
          1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055),
          c * 12.92,
          c <= vec3<f32>(0.0031308)
      );
  }
  fn RRTAndODTFit(v: vec3<f32>) -> vec3<f32> {
      let a = v * (v + 0.0245786) - 0.000090537;
      let b = v * (0.983729 * v + 0.4329510) + 0.238081;
      return a / b;
  }
  fn acesFilmic(color: vec3<f32>) -> vec3<f32> {
      let m1 = mat3x3<f32>(
          vec3<f32>(0.59719, 0.07600, 0.02840),
          vec3<f32>(0.35458, 0.90834, 0.13383),
          vec3<f32>(0.04823, 0.01566, 0.83777)
      );
      let m2 = mat3x3<f32>(
          vec3<f32>(1.60475, -0.10208, -0.00327),
          vec3<f32>(-0.53108, 1.10813, -0.07276),
          vec3<f32>(-0.07367, -0.00605, 1.07602)
      );
      var c = color / 0.6;
      c = m1 * c;
      c = RRTAndODTFit(c);
      c = m2 * c;
      return saturate(c);
  }

  fn hash_u32(x_in: u32) -> u32 {
      var a = x_in;
      a = (a ^ 61u) ^ (a >> 16u);
      a = a + (a << 3u);
      a = a ^ (a >> 4u);
      a = a * 0x27d4eb2du;
      a = a ^ (a >> 15u);
      return a;
  }
  fn hash_f32(x_in: u32) -> f32 {
      return f32(hash_u32(x_in)) / 4294967295.0;
  }

  fn start_pos(index: u32) -> vec3<f32> {
      let theta = hash_f32(index * 3u + 1u) * 6.2831853;
      let cos_phi = hash_f32(index * 7u + 13u) * 2.0 - 1.0;
      let sin_phi = sqrt(max(1.0 - cos_phi * cos_phi, 0.0));
      let dist = pow(hash_f32(index * 11u + 29u), 0.55) * BOUNDS * 0.75;
      return vec3<f32>(
          sin_phi * cos(theta) * dist,
          cos_phi * dist * 0.65,
          sin_phi * sin(theta) * dist
      );
  }

  fn hsl_to_rgb(h: f32, s: f32, l: f32) -> vec3<f32> {
      let c = (1.0 - abs(2.0 * l - 1.0)) * s;
      let hp = fract(h) * 6.0;
      let x = c * (1.0 - abs(hp % 2.0 - 1.0));
      var rgb = vec3<f32>(0.0);
      if      (hp < 1.0) { rgb = vec3<f32>(c, x, 0.0); }
      else if (hp < 2.0) { rgb = vec3<f32>(x, c, 0.0); }
      else if (hp < 3.0) { rgb = vec3<f32>(0.0, c, x); }
      else if (hp < 4.0) { rgb = vec3<f32>(0.0, x, c); }
      else if (hp < 5.0) { rgb = vec3<f32>(x, 0.0, c); }
      else               { rgb = vec3<f32>(c, 0.0, x); }
      return rgb + (l - c * 0.5);
  }

  fn rotate_vector(v: vec3<f32>, q: vec4<f32>) -> vec3<f32> {
      return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v);
  }

  fn axis_vec(i: u32) -> vec3<f32> {
      return vec3<f32>(f32(i == 0u), f32(i == 1u), f32(i == 2u));
  }

  @vertex
  fn vs_main(
      @location(0) vertex_pos: vec3<f32>,
      @location(1) vertex_normal: vec3<f32>,
      @location(2) vertex_uv:     vec2<f32>,
      @location(6) vertex_color:  vec4<f32>,
      @location(3) inst_pos: vec3<f32>,
      @location(4) inst_vel: vec3<f32>,
      @location(5) inst_scale: f32,
      @location(7) inst_agility: f32,
      @builtin(vertex_index) vert_idx: u32,

      @location(8) instance_idx: u32,
  ) -> VertexOutput {

      let a = hash_f32(instance_idx * 5u + 3u) * 2.0 - 1.0;
      let b = hash_f32(instance_idx * 9u + 19u) * 2.0 - 1.0;
      let c = hash_f32(instance_idx * 13u + 37u) * 2.0 - 1.0;
      let d = hash_f32(instance_idx * 23u + 53u) * 2.0 - 1.0;
      let q = normalize(vec4<f32>(a, b, c, d));
${VS_GEOMETRY[faces]}

      let fade_full = scene.exposure.z;
      let fade_zero = scene.exposure.w;
      var fade = 1.0;
      if (fade_full > fade_zero) {
          fade = smoothstep(fade_zero, fade_full, distance(inst_pos, scene.camera_pos.xyz));
      }
      let world_pos = inst_pos + r_pos * (inst_scale * fade);

      var out: VertexOutput;
      out.position = scene.view_proj * vec4<f32>(world_pos, 1.0);
      out.normal = normalize(r_norm);

      let sp = start_pos(instance_idx);
      let dist = length(sp);
      let hue = fract(atan2(sp.z, sp.x) / 6.2831853 + 0.5 + sp.y * 0.006);
      let lift = 0.45 + (dist / BOUNDS) * 0.3;

      let base = hsl_to_rgb(hue, 0.72, lift);
      out.color = vec4<f32>(pow(base, vec3<f32>(2.2)), 1.0);
      out.emissive = base * EMISSIVE;
      return out;
  }

  const INV_PI: f32 = 0.31830988618;

  @fragment
  fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
      let exposure = scene.exposure.x;
      let albedo = in.color.rgb;

      let ambient_light = srgbToLinear(scene.ambient_solid.rgb) * scene.ambient_solid.a;
      var lo = albedo * INV_PI * ambient_light;

      let l = normalize(-scene.dir0_dir.xyz);
      let ndotl = max(dot(normalize(in.normal), l), 0.0);
      let dir_col = srgbToLinear(scene.dir0_color.rgb) * scene.dir0_dir.w;
      lo += albedo * INV_PI * dir_col * ndotl;

      lo += in.emissive;

      let exposed = lo * exposure;
      let mapped = acesFilmic(exposed);
      let corrected = linearToSrgb(mapped);
      return vec4<f32>(corrected, 1.0);
  }
`;

class RollingAverage {
  constructor(size = 30) {
    this.size = size;
    this.samples = [];
  }
  add(v) {
    this.samples.push(v);
    if (this.samples.length > this.size) this.samples.shift();
  }
  get() {
    if (!this.samples.length) return 0;
    return this.samples.reduce((a, b) => a + b, 0) / this.samples.length;
  }
  reset() {
    this.samples.length = 0;
  }
}

function basisToQuat(right, up, forward, roll = 0) {
  let [rx, ry, rz] = right;
  let [ux, uy, uz] = up;
  const [fx, fy, fz] = forward;

  if (roll !== 0) {
    const c = Math.cos(roll), s = Math.sin(roll);
    const nrx = rx * c + ux * s, nry = ry * c + uy * s, nrz = rz * c + uz * s;
    ux = ux * c - rx * s; uy = uy * c - ry * s; uz = uz * c - rz * s;
    rx = nrx; ry = nry; rz = nrz;
  }

  const zx = -fx, zy = -fy, zz = -fz;
  const m00 = rx, m01 = ux, m02 = zx;
  const m10 = ry, m11 = uy, m12 = zy;
  const m20 = rz, m21 = uz, m22 = zz;

  const trace = m00 + m11 + m22;
  let x, y, z, w;
  if (trace > 0) {
    const s = Math.sqrt(trace + 1.0) * 2;
    w = 0.25 * s;
    x = (m21 - m12) / s;
    y = (m02 - m20) / s;
    z = (m10 - m01) / s;
  } else if (m00 > m11 && m00 > m22) {
    const s = Math.sqrt(1.0 + m00 - m11 - m22) * 2;
    w = (m21 - m12) / s;
    x = 0.25 * s;
    y = (m01 + m10) / s;
    z = (m02 + m20) / s;
  } else if (m11 > m22) {
    const s = Math.sqrt(1.0 + m11 - m00 - m22) * 2;
    w = (m02 - m20) / s;
    x = (m01 + m10) / s;
    y = 0.25 * s;
    z = (m12 + m21) / s;
  } else {
    const s = Math.sqrt(1.0 + m22 - m00 - m11) * 2;
    w = (m10 - m01) / s;
    x = (m02 + m20) / s;
    y = (m12 + m21) / s;
    z = 0.25 * s;
  }
  return [x, y, z, w];
}

async function start() {
  const wasm = await init();

  try {
    await initThreadPool(navigator.hardwareConcurrency);
  } catch (e) {
    console.warn("[murmuration] thread pool unavailable, running serial:", e);
  }

  const rayonThreads = artisan_rayon_threads();

  const canvas = document.getElementById("gameCanvas");

  const bootParams = new URLSearchParams(location.search);
  const msaa = parseInt(bootParams.get("msaa") || "4", 10);

  const faces = bootParams.get("faces") === "6" ? 6 : 3;
  const renderer = new WebGPURenderer(canvas, { msaa });
  await renderer.init();
  renderer.setClearColor(0.0, 0.0, 0.0, 1.0);
  renderer.registerGPUSimShader(
    SHADER_TYPE,
    FlowComputeWGSL,
    makeFlowRenderWGSL(faces),
  );

  const CUBE_RADIUS = CUBE_SCALE * Math.sqrt(3) * 0.5;
  renderer.renderer3D.setNearFade(2.0, Math.max(0.4, (0.1 + CUBE_RADIUS) * 2));

  const cubeData = mesh_cube(1.0);
  const cubeMeshId = renderer.assets.createMesh(cubeData.vertices, cubeData.indices);

  const hullIndices = new Uint32Array(18);
  for (let f = 0; f < 3; f++) {
    const b = f * 4;
    hullIndices.set([b, b + 1, b + 2, b + 2, b + 3, b], f * 6);
  }
  const hullMeshId = renderer.assets.createMesh(
    new Float32Array(12 * 12),
    hullIndices,
  );

  const gpuMeshId = faces === 3 ? hullMeshId : cubeMeshId;

  const params = new URLSearchParams(location.search);
  const touchDefaultCount = matchMedia("(pointer: coarse)").matches ? "25000" : "250000";
  let count = parseInt(params.get("count") || touchDefaultCount, 10);
  if (!Number.isFinite(count) || count < 1) count = Number(touchDefaultCount);
  let mode = params.get("mode") === "gpu" ? "gpu" : "cpu";

  let cullEnabled = params.get("cull") === "1";

  const camParam = params.get("cam");

  const uiParam = (params.get("ui") || "").toLowerCase();
  const startChromeHidden = ["0", "off", "false", "hide", "hidden", "none"].includes(
    uiParam,
  );

  let engine = null;
  let app = null;
  let camEntity = -1;
  let spawnMs = 0;

  const avgCpu = new RollingAverage();
  const avgUpload = new RollingAverage();
  const avgGpu = new RollingAverage();
  const avgFrame = new RollingAverage();

  const el = (id) => document.getElementById(id);

  const GPU_MAX_COUNT = Math.floor(
    renderer.device.limits.maxStorageBufferBindingSize / 32,
  );

  function buildScene() {
    let note = "";
    if (mode === "gpu" && count > GPU_MAX_COUNT) {
      note = ` — capped from ${count.toLocaleString()} (this GPU allows ${GPU_MAX_COUNT.toLocaleString()})`;
      count = GPU_MAX_COUNT;
    } else if (count >= 2000000) {
      note =
        mode === "cpu"
          ? " — CPU mode at this count may hang the tab for several seconds while spawning, and the tick itself will likely drop well under interactive framerate"
          : " — large GPU allocation, may be slow to spawn or fail outright depending on your GPU";
    }
    el("count-note").innerText = note;

    el("loading").style.display = "block";
    el("loading").innerText =
      mode === "cpu"
        ? `Spawning ${count.toLocaleString()} entities…`
        : `Allocating ${count.toLocaleString()} particles…`;

    return new Promise((resolve, reject) => {
      setTimeout(() => {
        try {

        for (const state of renderer.renderer3D.gpuSimStates.values()) {
          state.particleBuffer?.destroy();
          state.paramBuffer?.destroy();
        }
        renderer.renderer3D.gpuSimStates.clear();

        renderer.renderer3D.resetSimTime();

        renderer.renderer3D.setFrustumCulling(cullEnabled && mode === "gpu");

        const t0 = performance.now();
        engine =
          mode === "cpu"
            ? create_swarm_cpu(count, cubeMeshId, CUBE_SCALE)
            : create_swarm_gpu(count, gpuMeshId, CUBE_SCALE);
        spawnMs = performance.now() - t0;

        app = new ArtisanApp(engine, wasm.memory).registerStandardSchemas();

        app.input.setBlockHotkeys(false);

        const cams = app.world.query(["Camera3D", "Transform"]);
        camEntity = cams.length && cams[0].len ? cams[0].entities[0] : -1;

        camClock = 0;
        ridePos = [22, 5, 9];
        rideVel = [0, 0, 0];
        resetCameraFrame();

        avgCpu.reset();
        avgUpload.reset();
        avgGpu.reset();
        avgFrame.reset();

        el("loading").style.display = "none";

        window.__murmuration = {
          engine, app, renderer, updateCamera,
          get mode() { return mode; },
          get camMode() { return camMode; },
          get count() { return count; },
          get camClock() { return camClock; },
        };
          syncButtons();
          resolve();
        } catch (error) {
          el("loading").style.display = "none";
          reject(error);
        }
      }, 32);
    });
  }

  let camClock = 0;
  let camMode = camParam === "orbit" || camParam === "ride" ? camParam : "ride";
  let dragging = false;

  let pendingYaw = 0;
  let pendingPitch = 0;

  canvas.addEventListener("pointerdown", (e) => {
    dragging = true;
    canvas.setPointerCapture(e.pointerId);
  });
  canvas.addEventListener("pointerup", (e) => {
    dragging = false;
    canvas.releasePointerCapture(e.pointerId);
  });
  canvas.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    pendingYaw -= e.movementX * 0.005;
    pendingPitch -= e.movementY * 0.004;
  });

  window.addEventListener("keydown", (e) => {
    if (e.key !== "[" && e.key !== "]") return;
    const views = app.world.query(["AmbientLight"]);
    if (!views.length || !views[0].len) return;
    const amb = views[0].arrays["AmbientLight"];
    amb[3] = Math.max(0, amb[3] * (e.key === "]" ? 1.15 : 0.87));
    const note = el("hint");
    note.innerText = `ambient intensity: ${amb[3].toFixed(0)}`;
  });

  function toggleFullscreen() {
    if (!document.fullscreenElement) {
      document.documentElement.requestFullscreen?.().catch(() => {});
    } else {
      document.exitFullscreen?.();
    }
  }
  window.addEventListener("keydown", (e) => {
    if (e.key === "f" || e.key === "F") toggleFullscreen();
  });

  el("fullscreen").addEventListener("click", toggleFullscreen);

  const isTouch = matchMedia("(pointer: coarse)").matches;
  if (isTouch) {
    document.body.classList.add("touch");
    el("hint").innerText = "drag to look around - ⛶ fullscreen";
  }

  function dot3(a, b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
  function cross3(a, b) {
    return [
      a[1] * b[2] - a[2] * b[1],
      a[2] * b[0] - a[0] * b[2],
      a[0] * b[1] - a[1] * b[0],
    ];
  }
  function norm3(v) {
    const l = Math.hypot(v[0], v[1], v[2]) || 1;
    return [v[0] / l, v[1] / l, v[2] / l];
  }
  function sub3(a, b) { return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]; }
  function scale3(a, s) { return [a[0] * s, a[1] * s, a[2] * s]; }

  function rotateAroundAxis(v, axis, angle) {
    const c = Math.cos(angle), s = Math.sin(angle);
    const d = dot3(v, axis);
    const cr = cross3(axis, v);
    return [
      v[0] * c + cr[0] * s + axis[0] * d * (1 - c),
      v[1] * c + cr[1] * s + axis[1] * d * (1 - c),
      v[2] * c + cr[2] * s + axis[2] * d * (1 - c),
    ];
  }

  let camFrameReady = false;
  let curFwd = [0, 0, -1];
  let curRight = [1, 0, 0];

  const AIM_EASE_RATE = 0.35;

  function resetCameraFrame() {
    camFrameReady = false;
    pendingYaw = 0;
    pendingPitch = 0;
  }

  function stepCameraFrame(targetFwd, dt) {
    if (!camFrameReady) {
      curFwd = targetFwd.slice();
      const seed = Math.abs(curFwd[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0];
      curRight = norm3(cross3(curFwd, seed));
      camFrameReady = true;
    }

    if (pendingYaw !== 0 || pendingPitch !== 0) {
      const up = cross3(curRight, curFwd);
      curFwd = norm3(rotateAroundAxis(curFwd, up, pendingYaw));
      curRight = norm3(rotateAroundAxis(curRight, up, pendingYaw));
      curFwd = norm3(rotateAroundAxis(curFwd, curRight, pendingPitch));
      pendingYaw = 0;
      pendingPitch = 0;
    }

    const cosA = Math.max(-1, Math.min(1, dot3(curFwd, targetFwd)));
    const angle = Math.acos(cosA);
    if (angle > 1e-5) {
      const axis = norm3(cross3(curFwd, targetFwd));
      const step = angle * (1 - Math.exp(-AIM_EASE_RATE * dt));
      curFwd = norm3(rotateAroundAxis(curFwd, axis, step));
      curRight = norm3(rotateAroundAxis(curRight, axis, step));
    }

    curRight = norm3(sub3(curRight, scale3(curFwd, dot3(curRight, curFwd))));
    const curUp = cross3(curRight, curFwd);
    return basisToQuat(curRight, curUp, curFwd, 0);
  }

  const FIXED_EYE = [42, 15, 30];

  function orbitEye() {
    return FIXED_EYE;
  }

  const RIDE_FLOW_SPEED = 7.0;
  function rideFlowField(x, y, z, t) {
    const S = 0.09;
    let vx = Math.sin(y * S + t * 0.31) + Math.cos(z * S * 1.3 - t * 0.21);
    let vy = Math.sin(z * S * 1.1 + t * 0.27) + Math.cos(x * S * 0.9 + t * 0.19);
    let vz = Math.sin(x * S * 1.2 - t * 0.23) + Math.cos(y * S * 1.05 + t * 0.25);
    vx += -z * 0.05;
    vz += x * 0.05;
    const r = Math.hypot(x, y, z);
    const over = Math.max(r / BOUNDS - 0.6, 0);
    if (over > 0 && r > 0.001) {
      const pull = over * over * 3.0;
      vx -= (x / r) * pull;
      vy -= (y / r) * pull;
      vz -= (z / r) * pull;
    }
    return [vx, vy, vz];
  }
  let ridePos = [22, 5, 9];
  let rideVel = [0, 0, 0];
  let rideFwd = [0, 0, -1];

  function stepRide(dt) {
    const target = rideFlowField(ridePos[0], ridePos[1], ridePos[2], camClock);
    const blend = 1 - Math.exp(-dt * 2.5);
    for (let i = 0; i < 3; i++) {
      rideVel[i] += (target[i] * RIDE_FLOW_SPEED - rideVel[i]) * blend;
    }
    for (let i = 0; i < 3; i++) ridePos[i] += rideVel[i] * dt;
    const sl = Math.hypot(rideVel[0], rideVel[1], rideVel[2]);
    if (sl > 0.05) rideFwd = [rideVel[0] / sl, rideVel[1] / sl, rideVel[2] / sl];
  }

  function updateCamera(dt) {
    if (camEntity < 0) return;
    camClock += dt;
    stepRide(dt);

    let eye, baseFwd;
    if (camMode === "orbit") {
      eye = orbitEye(camClock);
      const dx = -eye[0], dy = -eye[1], dz = -eye[2];
      const dl = Math.hypot(dx, dy, dz) || 1;
      baseFwd = [dx / dl, dy / dl, dz / dl];
    } else {
      eye = ridePos;

      const dx = -eye[0], dy = -eye[1], dz = -eye[2];
      const dl = Math.hypot(dx, dy, dz) || 1;
      baseFwd = dl > 1e-3 ? [dx / dl, dy / dl, dz / dl] : rideFwd;
    }

    const q = stepCameraFrame(baseFwd, dt);

    const views = app.world.query(["Camera3D", "Transform"]);
    if (!views.length || !views[0].len) return;
    const tr = views[0].arrays["Transform"];
    tr[0] = eye[0]; tr[1] = eye[1]; tr[2] = eye[2];
    tr[3] = q[0]; tr[4] = q[1]; tr[5] = q[2]; tr[6] = q[3];
    app.world.wasm.wasm_mark_changed(camEntity, app.world.schemas["Transform"].id);

    const cam = views[0].arrays["Camera3D"];
    cam[1] = canvas.width / Math.max(canvas.height, 1);
  }

  function syncButtons() {
    for (const b of document.querySelectorAll("[data-count]")) {
      b.classList.toggle("active", parseInt(b.dataset.count, 10) === count);
    }
    for (const b of document.querySelectorAll("[data-mode]")) {
      b.classList.toggle("active", b.dataset.mode === mode);
    }
    for (const b of document.querySelectorAll("[data-cam]")) {
      b.classList.toggle("active", b.dataset.cam === camMode);
    }
    for (const b of document.querySelectorAll("[data-cull]")) {
      b.classList.toggle("active", (b.dataset.cull === "on") === cullEnabled);
    }
    el("row-cpu-label").innerText =
      mode === "cpu" ? "ECS tick (simulate all)" : "ECS tick (1 entity)";
  }

  const panel = el("ui");
  const toggle = el("toggle");
  const hint = el("hint");
  let chromeHidden = startChromeHidden;
  function setPanel(open) {
    panel.hidden = !open;
    toggle.style.display = open || chromeHidden ? "none" : "block";
    hint.style.display = chromeHidden ? "none" : "block";
  }
  toggle.addEventListener("click", () => setPanel(true));
  el("close").addEventListener("click", () => setPanel(false));
  window.addEventListener("keydown", (e) => {
    if (e.key !== "h" && e.key !== "H") return;

    if (chromeHidden) {
      chromeHidden = false;
      setPanel(true);
    } else {
      setPanel(panel.hidden);
    }
  });
  setPanel(false);

  for (const b of document.querySelectorAll("[data-count]")) {
    b.addEventListener("click", async () => {
      count = parseInt(b.dataset.count, 10);
      await buildScene();
    });
  }
  for (const b of document.querySelectorAll("[data-mode]")) {
    b.addEventListener("click", async () => {
      mode = b.dataset.mode;
      await buildScene();
    });
  }

  for (const b of document.querySelectorAll("[data-cull]")) {
    b.addEventListener("click", () => {
      cullEnabled = b.dataset.cull === "on";
      renderer.renderer3D.setFrustumCulling(cullEnabled);
      syncButtons();
    });
  }
  for (const b of document.querySelectorAll("[data-cam]")) {
    b.addEventListener("click", () => {
      camMode = b.dataset.cam;

      resetCameraFrame();
      syncButtons();
    });
  }

  await buildScene();

  let last = performance.now();
  let fpsLast = last;
  let frames = 0;
  let fps = 0;

  const loop = (now) => {

    try {
      const dt = Math.min((now - last) / 1000, 0.05);
      last = now;
      frames++;
      if (now - fpsLast >= 500) {
        fps = (frames * 1000) / (now - fpsLast);
        frames = 0;
        fpsLast = now;
      }

      if (renderer.deviceLost) {
        const note = el("hint");

        note.style.display = "block";
        note.innerText = `GPU device lost (${renderer.deviceLost.reason}) — reload to restart`;
        return;
      }

      updateCamera(dt);

      const t0 = performance.now();
      engine.tick(dt);
      const cpuMs = performance.now() - t0;

      renderer.render3D(app.world, dt);

      const stats = renderer.renderer3D.lastStats;
      avgCpu.add(cpuMs);
      avgUpload.add(stats.writeBufferTimeMs);
      avgGpu.add(stats.gpuExecutionTimeMs);
      avgFrame.add(dt * 1000);

      if (!panel.hidden) {
        el("v-count").innerText = count.toLocaleString();
        el("v-fps").innerText = fps.toFixed(0);
        el("v-cpu").innerText = `${avgCpu.get().toFixed(2)} ms`;
        el("v-gpu").innerText = `${avgGpu.get().toFixed(2)} ms`;
        el("v-frame").innerText = `${avgFrame.get().toFixed(1)} ms`;

        el("v-threads").innerText =
          mode === "cpu" ? String(rayonThreads) : `${rayonThreads} (idle)`;

        const cs = renderer.renderer3D.lastCullStats;
        el("v-drawn").innerText =
          cullEnabled && mode === "gpu"
            ? `${cs.drawn.toLocaleString()} (${((100 * cs.drawn) / Math.max(count, 1)).toFixed(0)}%)`
            : count.toLocaleString();
      }
    } catch (err) {
      console.error("[murmuration] frame error (continuing):", err);
    } finally {
      requestAnimationFrame(loop);
    }
  };
  requestAnimationFrame(loop);
}

start().catch((e) => {
  const loading = document.getElementById("loading");
  const insecureWebGPU = !window.isSecureContext && !navigator.gpu;
  loading.style.display = "flex";
  loading.style.padding = "2rem";
  loading.style.textAlign = "center";
  loading.style.lineHeight = "1.5";
  loading.style.pointerEvents = "none";
  loading.innerText = insecureWebGPU
    ? "WebGPU is blocked because this LAN page uses HTTP. Open it through HTTPS, or mark this development origin as secure in Chrome flags."
    : `Failed to start: ${e.message}`;

  const panel = document.getElementById("ui");
  const toggle = document.getElementById("toggle");
  toggle.addEventListener("click", () => {
    panel.hidden = false;
    toggle.style.display = "none";
  });
  document.getElementById("close").addEventListener("click", () => {
    panel.hidden = true;
    toggle.style.display = "block";
  });
  console.error(e);
});
