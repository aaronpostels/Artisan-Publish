import { Shaders3D } from "./Shaders3D.js";

const SIM_WORKGROUP_SIZE = 64;

const MAX_DISPATCH_PER_DIM = 65535;

export class Renderer3D {
  constructor(renderer) {
    this.renderer = renderer;
    this.device = null;
    this.assets = null;
    this.maxInstances3D = 0;
    this.sceneBuffer3D = null;
    this.sceneBindGroupLayout3D = null;
    this.sceneBindGroup3D = null;
    this.transformBuffers3D = [];
    this.materialBuffers3D = [];
    this.frameIndex = 0;
    this.materialNeedsUpload = [true, true, true];
    this.lastStructuralGen = -1;
    this.lastMemoryBuffer = null;
    this.cachedMemView = null;
    this.lightBuffer3D = null;
    this.materialRegistry = new Map();
    this.skyPipeline = null;
    this.gridPipeline = null;
    this.gizmoPipeline = null;
    this.editorMode = false;
    this.selectedEntity = null;
    this.gpuSimStates = new Map();
    this.gpuSimShaders = new Map();
    this.lastFrameTime = null;

    this.simTime = 0;

    this.nearFadeFull = 0;
    this.nearFadeZero = 0;
    this.lastStats = {
      batches: 0,
      instances: 0,
      uploadTimeMs: 0,
      passTimeMs: 0,
      writeBufferTimeMs: 0,
      computeRecordTimeMs: 0,
      renderRecordTimeMs: 0,
      gpuExecutionTimeMs: 0,

      gpuRenderPassMs: 0,
      gpuComputePassMs: 0,
      gpuCullPassMs: 0,
    };

    this.timestampQuerySet = null;
    this.timestampResolveBuffer = null;

    this.timestampReadBuffers = [];
    this.timestampsEnabled = false;

    this.frustumCulling = false;

    this.transformStaging = [];
    this.materialStaging = [];

    this.stagingCopies = [];
    this.cullPlanes = new Float32Array(24);
    this.cullParamsHost = new Float32Array(28);
    this.indirectHost = new Uint32Array(5);
    this.lastCullStats = { submitted: 0, drawn: 0 };
  }

  enableGPUTimestamps(on = true) {
    if (!on) {
      this.timestampsEnabled = false;
      return false;
    }
    if (!this.device?.features?.has("timestamp-query")) return false;
    if (!this.timestampQuerySet) {

      this.timestampQuerySet = this.device.createQuerySet({
        type: "timestamp",
        count: 6,
      });
      this.timestampResolveBuffer = this.device.createBuffer({
        size: 6 * 8,
        usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
      });
      for (let i = 0; i < 3; i++) {
        this.timestampReadBuffers.push({
          buffer: this.device.createBuffer({
            size: 6 * 8,
            usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
          }),
          busy: false,
        });
      }
    }
    this.timestampsEnabled = true;
    return true;
  }

  markMaterialsDirty() {
    this.materialNeedsUpload.fill(true);
  }

  acquireStaging(pool, bytes) {
    const slot = pool.find((s) => s.mapped && s.buffer.size >= bytes);
    if (!slot) return null;
    try {
      const view = new Uint8Array(slot.buffer.getMappedRange(0, bytes));
      slot.mapped = false;
      return { slot, view, bytes };
    } catch (e) {

      slot.mapped = false;
      return null;
    }
  }

  flushStaging(claim, dst) {
    claim.slot.buffer.unmap();
    this.stagingCopies.push({
      slot: claim.slot,
      dst,
      bytes: claim.bytes,
    });
  }

  ensureStagingPool(pool, bytes) {

    const size = Math.max(1 << 16, 1 << Math.ceil(Math.log2(bytes)));
    for (let i = 0; i < 3; i++) {
      const slot = pool[i];
      if (slot && slot.buffer.size >= size) continue;
      if (slot) {

        if (slot.mapped) slot.buffer.unmap();
        slot.buffer.destroy();
      }
      const buffer = this.device.createBuffer({
        size,
        usage: GPUBufferUsage.MAP_WRITE | GPUBufferUsage.COPY_SRC,
      });
      pool[i] = { buffer, mapped: false };
      const created = pool[i];
      buffer.mapAsync(GPUMapMode.WRITE).then(
        () => {
          created.mapped = true;
        },
        () => {},
      );
    }
    return pool;
  }

  remapStagingSlots() {
    for (const { slot } of this.stagingCopies) {
      slot.buffer.mapAsync(GPUMapMode.WRITE).then(
        () => {
          slot.mapped = true;
        },
        () => {},
      );
    }
    this.stagingCopies.length = 0;
  }

  setNearFade(full, zero) {
    this.nearFadeFull = full;
    this.nearFadeZero = zero;
  }

  resetSimTime() {
    this.simTime = 0;
    this.lastFrameTime = null;
  }

  registerGPUSimShader(typeId, computeWgsl, renderWgsl) {
    this.gpuSimShaders.set(typeId, { computeWgsl, renderWgsl });
  }

  setFrustumCulling(on) {
    this.frustumCulling = !!on;
  }

  static frustumPlanes(m, out) {
    const rows = [
      [m[0], m[4], m[8], m[12]],
      [m[1], m[5], m[9], m[13]],
      [m[2], m[6], m[10], m[14]],
      [m[3], m[7], m[11], m[15]],
    ];
    const [rx, ry, rz, rw] = rows;
    const planes = [
      [rw[0] + rx[0], rw[1] + rx[1], rw[2] + rx[2], rw[3] + rx[3]],
      [rw[0] - rx[0], rw[1] - rx[1], rw[2] - rx[2], rw[3] - rx[3]],
      [rw[0] + ry[0], rw[1] + ry[1], rw[2] + ry[2], rw[3] + ry[3]],
      [rw[0] - ry[0], rw[1] - ry[1], rw[2] - ry[2], rw[3] - ry[3]],
      [rz[0], rz[1], rz[2], rz[3]],
      [rw[0] - rz[0], rw[1] - rz[1], rw[2] - rz[2], rw[3] - rz[3]],
    ];
    for (let i = 0; i < 6; i++) {
      const p = planes[i];
      const len = Math.hypot(p[0], p[1], p[2]) || 1;
      out[i * 4 + 0] = p[0] / len;
      out[i * 4 + 1] = p[1] / len;
      out[i * 4 + 2] = p[2] / len;
      out[i * 4 + 3] = p[3] / len;
    }
    return out;
  }

  init() {
    this.device = this.renderer.device;
    this.assets = this.renderer.assets;
    this.sceneBuffer3D = this.device.createBuffer({
      size: 352,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.lightBuffer3D = this.device.createBuffer({
      size: 1024 * 32,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });
    this.sceneBindGroupLayout3D = this.device.createBindGroupLayout({
      entries: [
        {
          binding: 0,
          visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          buffer: { type: "uniform" },
        },
        {
          binding: 1,
          visibility: GPUShaderStage.FRAGMENT,
          buffer: { type: "read-only-storage" },
        },
      ],
    });
    this.sceneBindGroup3D = this.device.createBindGroup({
      layout: this.sceneBindGroupLayout3D,
      entries: [
        { binding: 0, resource: { buffer: this.sceneBuffer3D } },
        { binding: 1, resource: { buffer: this.lightBuffer3D } },
      ],
    });

    this.createShader(0, Shaders3D.SharedWGSL + Shaders3D.StandardFS, true);

    this.skyPipeline = this.device.createRenderPipeline({
      layout: this.device.createPipelineLayout({
        bindGroupLayouts: [
          this.device.createBindGroupLayout({
            entries: [
              {
                binding: 0,
                visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
                buffer: { type: "uniform" },
              },
            ],
          }),
        ],
      }),
      vertex: {
        module: this.device.createShaderModule({ code: Shaders3D.SkyWGSL }),
        entryPoint: "vs_main",
      },
      fragment: {
        module: this.device.createShaderModule({ code: Shaders3D.SkyWGSL }),
        entryPoint: "fs_main",
        targets: [{ format: this.renderer.gpuFormat }],
      },
      depthStencil: {
        depthWriteEnabled: false,
        depthCompare: "always",
        format: "depth24plus",
      },
      multisample: { count: this.renderer.msaaCount },
      primitive: { topology: "triangle-list" },
    });

    this.gridPipeline = this.device.createRenderPipeline({
      layout: this.device.createPipelineLayout({
        bindGroupLayouts: [
          this.device.createBindGroupLayout({
            entries: [
              {
                binding: 0,
                visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
                buffer: { type: "uniform" },
              },
            ],
          }),
        ],
      }),
      vertex: {
        module: this.device.createShaderModule({ code: Shaders3D.GridWGSL }),
        entryPoint: "vs_main",
      },
      fragment: {
        module: this.device.createShaderModule({ code: Shaders3D.GridWGSL }),
        entryPoint: "fs_main",
        targets: [
          {
            format: this.renderer.gpuFormat,
            blend: {
              color: {
                srcFactor: "src-alpha",
                dstFactor: "one-minus-src-alpha",
                operation: "add",
              },
              alpha: {
                srcFactor: "one",
                dstFactor: "one-minus-src-alpha",
                operation: "add",
              },
            },
          },
        ],
      },
      depthStencil: {
        depthWriteEnabled: false,
        depthCompare: "less",
        format: "depth24plus",
      },
      multisample: { count: this.renderer.msaaCount },
      primitive: { topology: "triangle-list" },
    });

    this.gizmoPipeline = this.device.createRenderPipeline({
      layout: this.device.createPipelineLayout({
        bindGroupLayouts: [
          this.device.createBindGroupLayout({
            entries: [
              {
                binding: 0,
                visibility: GPUShaderStage.VERTEX,
                buffer: { type: "uniform" },
              },
            ],
          }),
          this.device.createBindGroupLayout({
            entries: [
              {
                binding: 0,
                visibility: GPUShaderStage.VERTEX,
                buffer: { type: "uniform" },
              },
            ],
          }),
        ],
      }),
      vertex: {
        module: this.device.createShaderModule({ code: Shaders3D.GizmoWGSL }),
        entryPoint: "vs_main",
      },
      fragment: {
        module: this.device.createShaderModule({ code: Shaders3D.GizmoWGSL }),
        targets: [{ format: this.renderer.gpuFormat }],
      },
      depthStencil: {
        depthWriteEnabled: false,
        depthCompare: "always",
        format: "depth24plus",
      },
      multisample: { count: this.renderer.msaaCount },
      primitive: { topology: "line-list" },
    });

    this.gizmoBuffer = this.device.createBuffer({
      size: 16,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.gizmoBindGroup = this.device.createBindGroup({
      layout: this.gizmoPipeline.getBindGroupLayout(1),
      entries: [{ binding: 0, resource: { buffer: this.gizmoBuffer } }],
    });
  }

  createShader(id, wgsl, useDepth = true) {
    const pipelineLayout3D = this.device.createPipelineLayout({
      bindGroupLayouts: [this.sceneBindGroupLayout3D],
    });
    const createPipelineWithBlend = (blendConfig, depthWrite) => {
      return this.device.createRenderPipeline({
        layout: pipelineLayout3D,
        vertex: {
          module: this.device.createShaderModule({ code: wgsl }),
          entryPoint: "vs_main",
          buffers: [
            {
              arrayStride: 48,
              stepMode: "vertex",
              attributes: [
                { shaderLocation: 0, offset: 0, format: "float32x3" },
                { shaderLocation: 1, offset: 12, format: "float32x3" },
                { shaderLocation: 2, offset: 24, format: "float32x2" },
                { shaderLocation: 6, offset: 32, format: "float32x4" },
              ],
            },
            {
              arrayStride: 24,
              stepMode: "instance",
              attributes: [
                { shaderLocation: 3, offset: 0, format: "float32x3" },
                { shaderLocation: 4, offset: 12, format: "snorm16x4" },
                { shaderLocation: 5, offset: 20, format: "float32" },
              ],
            },
            {
              arrayStride: 48,
              stepMode: "instance",
              attributes: [
                { shaderLocation: 7, offset: 0, format: "float32x4" },
                { shaderLocation: 8, offset: 16, format: "float32x3" },
                { shaderLocation: 9, offset: 28, format: "float32" },
                { shaderLocation: 10, offset: 32, format: "float32" },
                { shaderLocation: 11, offset: 36, format: "float32x3" },
              ],
            },
          ],
        },
        fragment: {
          module: this.device.createShaderModule({ code: wgsl }),
          entryPoint: "fs_main",
          targets: [
            {
              format: this.renderer.gpuFormat,
              blend: blendConfig,
            },
          ],
        },
        depthStencil: {
          depthWriteEnabled: depthWrite,
          depthCompare: useDepth ? "less" : "always",
          format: "depth24plus",
        },
        multisample: { count: this.renderer.msaaCount },
        primitive: {
          topology: "triangle-list",
          cullMode: depthWrite ? "back" : "none",
        },
      });
    };
    const blendOpaque = {
      color: { srcFactor: "one", dstFactor: "zero", operation: "add" },
      alpha: { srcFactor: "one", dstFactor: "zero", operation: "add" },
    };
    const blendAlpha = {
      color: {
        srcFactor: "src-alpha",
        dstFactor: "one-minus-src-alpha",
        operation: "add",
      },
      alpha: {
        srcFactor: "one",
        dstFactor: "one-minus-src-alpha",
        operation: "add",
      },
    };
    const blendAdditive = {
      color: { srcFactor: "src-alpha", dstFactor: "one", operation: "add" },
      alpha: { srcFactor: "one", dstFactor: "one", operation: "add" },
    };
    const pipelines = {
      opaque: createPipelineWithBlend(blendOpaque, useDepth),
      transparent: createPipelineWithBlend(blendAlpha, false),
      additive: createPipelineWithBlend(blendAdditive, false),
    };
    this.materialRegistry.set(id, pipelines);
  }

  ensureBufferSize3D(count) {
    if (count <= this.maxInstances3D) return;
    for (const buf of this.transformBuffers3D) {
      if (buf) buf.destroy();
    }
    for (const buf of this.materialBuffers3D) {
      if (buf) buf.destroy();
    }
    this.transformBuffers3D = [];
    this.materialBuffers3D = [];
    this.maxInstances3D = Math.max(count, 1000);
    for (let i = 0; i < 3; i++) {
      this.transformBuffers3D.push(
        this.device.createBuffer({
          size: this.maxInstances3D * 24,
          usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
        }),
      );
      this.materialBuffers3D.push(
        this.device.createBuffer({
          size: this.maxInstances3D * 48,
          usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
        }),
      );
    }
    this.materialNeedsUpload.fill(true);
  }

  static CullWGSL = `
    struct Particle {
        pos_x: f32, pos_y: f32, pos_z: f32,
        vel_x: f32, vel_y: f32, vel_z: f32,
        scale: f32,
        agility: f32,
    };

    struct DrawArgs {
        index_count: u32,
        instance_count: atomic<u32>,
        first_index: u32,
        base_vertex: u32,
        first_instance: u32,
    };
    struct CullParams {
        planes: array<vec4<f32>, 6>,
        count: u32,
        radius_scale: f32,
        row_stride: f32,
        pad: f32,
    };
    @group(0) @binding(0) var<storage, read>       src:  array<Particle>;
    @group(0) @binding(1) var<storage, read_write> dst:  array<Particle>;
    @group(0) @binding(2) var<storage, read_write> draw: DrawArgs;
    @group(0) @binding(3) var<storage, read_write> orig: array<u32>;
    @group(0) @binding(4) var<uniform>             cp:   CullParams;

    @compute @workgroup_size(64)
    fn cs_cull(@builtin(global_invocation_id) id: vec3<u32>) {
        let i = id.x + id.y * u32(cp.row_stride);
        if (i >= cp.count) { return; }
        let p = src[i];
        let c = vec3<f32>(p.pos_x, p.pos_y, p.pos_z);
        let r = p.scale * cp.radius_scale;
        for (var k = 0u; k < 6u; k = k + 1u) {
            let pl = cp.planes[k];
            if (dot(pl.xyz, c) + pl.w < -r) { return; }
        }

        let o = atomicAdd(&draw.instance_count, 1u);
        dst[o] = p;
        orig[o] = i;
    }
  `;

  initGPUSimCulling(state) {
    if (state.cullPipeline) return state;
    const n = state.maxInstances;
    state.visibleBuffer = this.device.createBuffer({
      size: n * 32,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.VERTEX,
    });
    state.origIndexBuffer = this.device.createBuffer({
      size: n * 4,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.VERTEX,
    });
    state.indirectBuffer = this.device.createBuffer({
      size: 20,
      usage:
        GPUBufferUsage.INDIRECT |
        GPUBufferUsage.STORAGE |
        GPUBufferUsage.COPY_DST |

        GPUBufferUsage.COPY_SRC,
    });

    state.cullReadBuffers = [];
    for (let i = 0; i < 3; i++) {
      state.cullReadBuffers.push({
        buffer: this.device.createBuffer({
          size: 8,
          usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
        }),
        busy: false,
      });
    }
    state.cullParamBuffer = this.device.createBuffer({
      size: 112,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    const module = this.device.createShaderModule({
      code: Renderer3D.CullWGSL,
    });
    const layout = this.device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
        { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
        { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
        { binding: 3, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
        { binding: 4, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
      ],
    });
    state.cullPipeline = this.device.createComputePipeline({
      layout: this.device.createPipelineLayout({ bindGroupLayouts: [layout] }),
      compute: { module, entryPoint: "cs_cull" },
    });
    state.cullBindGroup = this.device.createBindGroup({
      layout,
      entries: [
        { binding: 0, resource: { buffer: state.particleBuffer } },
        { binding: 1, resource: { buffer: state.visibleBuffer } },
        { binding: 2, resource: { buffer: state.indirectBuffer } },
        { binding: 3, resource: { buffer: state.origIndexBuffer } },
        { binding: 4, resource: { buffer: state.cullParamBuffer } },
      ],
    });
    return state;
  }

  initGPUSim(entId, maxInstances, shaderType) {
    const particleSize = 32;
    const bufferSize = maxInstances * particleSize;
    const particleBuffer = this.device.createBuffer({
      size: bufferSize,
      usage:
        GPUBufferUsage.STORAGE |
        GPUBufferUsage.VERTEX |
        GPUBufferUsage.COPY_DST |

        GPUBufferUsage.COPY_SRC,
    });

    const CHUNK = 1 << 20;
    const staging = new Float32Array(Math.min(maxInstances, CHUNK) * 8);
    for (let base = 0; base < maxInstances; base += CHUNK) {
      const n = Math.min(CHUNK, maxInstances - base);
      for (let i = 0; i < n; i++) {
        const idx = i * 8;
        const theta = Math.random() * Math.PI * 2;
        const phi = Math.acos(Math.random() * 2 - 1);
        const dist = Math.random() * 15.0 + 5.0;
        staging[idx] = Math.sin(phi) * Math.cos(theta) * dist;
        staging[idx + 1] = Math.sin(phi) * Math.sin(theta) * dist;
        staging[idx + 2] = Math.cos(phi) * dist;
        staging[idx + 3] = (Math.random() - 0.5) * 5.0;
        staging[idx + 4] = (Math.random() - 0.5) * 5.0;
        staging[idx + 5] = (Math.random() - 0.5) * 5.0;
        staging[idx + 6] = 1.0;
        staging[idx + 7] = Math.random();
      }
      this.device.queue.writeBuffer(
        particleBuffer,
        base * particleSize,
        staging,
        0,
        n * 8,
      );
    }
    const paramBuffer = this.device.createBuffer({
      size: 32,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    const identityIndexBuffer = this.device.createBuffer({
      size: maxInstances * 4,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });
    {
      const CHUNK = 1 << 20;
      const idStaging = new Uint32Array(Math.min(maxInstances, CHUNK));
      for (let base = 0; base < maxInstances; base += CHUNK) {
        const n = Math.min(CHUNK, maxInstances - base);
        for (let i = 0; i < n; i++) idStaging[i] = base + i;
        this.device.queue.writeBuffer(
          identityIndexBuffer,
          base * 4,
          idStaging,
          0,
          n,
        );
      }
    }

    const customShaders = this.gpuSimShaders.get(shaderType);
    const computeShaderWGSL = customShaders
      ? customShaders.computeWgsl
      : Shaders3D.GPUSimComputeWGSL;
    const renderShaderWGSL = customShaders
      ? customShaders.renderWgsl
      : Shaders3D.GPUSimRenderWGSL;

    const computeModule = this.device.createShaderModule({
      code: computeShaderWGSL,
    });
    const computeBindGroupLayout = this.device.createBindGroupLayout({
      entries: [
        {
          binding: 0,
          visibility: GPUShaderStage.COMPUTE,
          buffer: { type: "storage" },
        },
        {
          binding: 1,
          visibility: GPUShaderStage.COMPUTE,
          buffer: { type: "uniform" },
        },
      ],
    });
    const computePipeline = this.device.createComputePipeline({
      layout: this.device.createPipelineLayout({
        bindGroupLayouts: [computeBindGroupLayout],
      }),
      compute: {
        module: computeModule,
        entryPoint: "cs_main",
      },
    });
    const computeBindGroup = this.device.createBindGroup({
      layout: computeBindGroupLayout,
      entries: [
        { binding: 0, resource: { buffer: particleBuffer } },
        { binding: 1, resource: { buffer: paramBuffer } },
      ],
    });
    const renderModule = this.device.createShaderModule({
      code: renderShaderWGSL,
    });
    const renderPipeline = this.device.createRenderPipeline({
      layout: this.device.createPipelineLayout({
        bindGroupLayouts: [this.sceneBindGroupLayout3D],
      }),
      vertex: {
        module: renderModule,
        entryPoint: "vs_main",
        buffers: [
          {
            arrayStride: 48,
            stepMode: "vertex",
            attributes: [
              { shaderLocation: 0, offset: 0, format: "float32x3" },
              { shaderLocation: 1, offset: 12, format: "float32x3" },
              { shaderLocation: 2, offset: 24, format: "float32x2" },
              { shaderLocation: 6, offset: 32, format: "float32x4" },
            ],
          },
          {
            arrayStride: 32,
            stepMode: "instance",
            attributes: [
              { shaderLocation: 3, offset: 0, format: "float32x3" },
              { shaderLocation: 4, offset: 12, format: "float32x3" },
              { shaderLocation: 5, offset: 24, format: "float32" },
              { shaderLocation: 7, offset: 28, format: "float32" },
            ],
          },

          {
            arrayStride: 4,
            stepMode: "instance",
            attributes: [{ shaderLocation: 8, offset: 0, format: "uint32" }],
          },
        ],
      },
      fragment: {
        module: renderModule,
        entryPoint: "fs_main",
        targets: [
          {
            format: this.renderer.gpuFormat,
          },
        ],
      },
      depthStencil: {
        depthWriteEnabled: true,
        depthCompare: "less",
        format: "depth24plus",
      },
      multisample: { count: this.renderer.msaaCount },
      primitive: { topology: "triangle-list", cullMode: "back" },
    });
    const state = {
      particleBuffer,
      paramBuffer,
      identityIndexBuffer,
      computePipeline,
      computeBindGroup,
      renderPipeline,
      maxInstances,
    };
    this.gpuSimStates.set(entId, state);
    return state;
  }

  render(world, appDt) {
    const now = performance.now();
    const measured = this.lastFrameTime
      ? (now - this.lastFrameTime) / 1000.0
      : 0.016;
    this.lastFrameTime = now;
    const dt = Number.isFinite(appDt) && appDt > 0 ? appDt : measured;

    this.simTime += Math.min(dt, 0.05);

    const t0 = now;
    const camViews = world.query(["Camera3D", "GlobalTransform"]);
    const sceneData = new Float32Array(88);
    sceneData[0] = 1;
    sceneData[5] = 1;
    sceneData[10] = 1;
    sceneData[15] = 1;
    sceneData[16] = 1;
    sceneData[21] = 1;
    sceneData[26] = 1;
    sceneData[31] = 1;
    let exposure = 1.0;
    if (camViews.length > 0 && camViews[0].len > 0) {
      const cam = camViews[0].arrays["Camera3D"];
      sceneData.set(cam.subarray(4, 20), 0);
      sceneData.set(cam.subarray(20, 36), 16);
      sceneData.set(cam.subarray(36, 39), 32);
      exposure = cam[39];
    }
    let ambCol = [0.0, 0.0, 0.0],
      ambLux = 0.0;
    const ambViews = world.query(["AmbientLight"]);
    if (ambViews.length > 0 && ambViews[0].len > 0) {
      const a = ambViews[0].arrays["AmbientLight"];
      ambCol = [a[0], a[1], a[2]];
      ambLux = a[3];
    }
    let hemiSkyCol = [0.0, 0.0, 0.0],
      hemiSkyLux = 0.0;
    let hemiGroundCol = [0.0, 0.0, 0.0],
      hemiGroundLux = 0.0;
    const hemiViews = world.query(["HemisphereLight"]);
    if (hemiViews.length > 0 && hemiViews[0].len > 0) {
      const h = hemiViews[0].arrays["HemisphereLight"];
      hemiSkyCol = [h[0], h[1], h[2]];
      hemiSkyLux = h[3];
      hemiGroundCol = [h[4], h[5], h[6]];
      hemiGroundLux = h[7];
    }
    const lightData = world.wasm.wasm_get_light_data();
    const activeLightsCount = lightData.length / 8;
    sceneData[35] = activeLightsCount;
    sceneData[36] = hemiSkyCol[0];
    sceneData[37] = hemiSkyCol[1];
    sceneData[38] = hemiSkyCol[2];
    sceneData[39] = hemiSkyLux;
    sceneData[40] = hemiGroundCol[0];
    sceneData[41] = hemiGroundCol[1];
    sceneData[42] = hemiGroundCol[2];
    sceneData[43] = hemiGroundLux;
    sceneData[44] = ambCol[0];
    sceneData[45] = ambCol[1];
    sceneData[46] = ambCol[2];
    sceneData[47] = ambLux;
    sceneData[48] = exposure;
    sceneData[49] = this.simTime;
    sceneData[50] = this.nearFadeFull;
    sceneData[51] = this.nearFadeZero;
    sceneData.fill(0.0, 52, 88);
    const lightViews = world.query(["DirectionalLight"]);
    let dirLightCount = 0;
    for (let i = 0; i < lightViews.length && dirLightCount < 4; i++) {
      const view = lightViews[i];
      const arr = view.arrays["DirectionalLight"];
      for (let j = 0; j < view.len && dirLightCount < 4; j++) {
        const r = arr[j * 8 + 0];
        const g = arr[j * 8 + 1];
        const b = arr[j * 8 + 2];
        const intensity = arr[j * 8 + 3];
        const dx = arr[j * 8 + 4];
        const dy = arr[j * 8 + 5];
        const dz = arr[j * 8 + 6];
        const len = Math.sqrt(dx * dx + dy * dy + dz * dz);
        const nx = len > 0 ? dx / len : 0.0;
        const ny = len > 0 ? dy / len : -1.0;
        const nz = len > 0 ? dz / len : 0.0;
        const offset = 52 + dirLightCount * 8;
        sceneData[offset + 0] = nx;
        sceneData[offset + 1] = ny;
        sceneData[offset + 2] = nz;
        sceneData[offset + 3] = intensity;
        sceneData[offset + 4] = r;
        sceneData[offset + 5] = g;
        sceneData[offset + 6] = b;
        sceneData[offset + 7] = 1.0;
        dirLightCount++;
      }
    }
    this.device.queue.writeBuffer(this.sceneBuffer3D, 0, sceneData);
    if (lightData.length > 0)
      this.device.queue.writeBuffer(this.lightBuffer3D, 0, lightData);
    const batches = world.wasm.wasm_get_render_batches_3d();
    const batchCount = batches.length / 5;
    let totalInstances = 0;
    for (let i = 0; i < batchCount; i++) totalInstances += batches[i * 5 + 4];
    this.ensureBufferSize3D(totalInstances);
    const currentGen = world.wasm.get_structural_gen();
    if (currentGen !== this.lastStructuralGen) {
      this.lastStructuralGen = currentGen;
      this.materialNeedsUpload.fill(true);
    }
    const activeTransformBuffer = this.transformBuffers3D[this.frameIndex];
    const activeMaterialBuffer = this.materialBuffers3D[this.frameIndex];
    if (totalInstances > 0) {
      const mem = world.memory.buffer;
      const wantMaterials = this.materialNeedsUpload[this.frameIndex];

      this.ensureStagingPool(this.transformStaging, totalInstances * 24);
      const tClaim = this.acquireStaging(
        this.transformStaging,
        totalInstances * 24,
      );
      let mClaim = null;
      if (wantMaterials) {
        this.ensureStagingPool(this.materialStaging, totalInstances * 48);
        mClaim = this.acquireStaging(
          this.materialStaging,
          totalInstances * 48,
        );
      }

      let offset = 0;
      for (let i = 0; i < batchCount; i++) {
        const gtPtr = batches[i * 5 + 0];
        const matPtr = batches[i * 5 + 1];
        const count = batches[i * 5 + 4];

        const transformSlice = new Uint8Array(mem, gtPtr, count * 24);
        if (tClaim) {
          tClaim.view.set(transformSlice, offset * 24);
        } else {
          this.device.queue.writeBuffer(
            activeTransformBuffer,
            offset * 24,
            transformSlice,
          );
        }
        if (wantMaterials) {
          const materialSlice = new Uint8Array(mem, matPtr, count * 48);
          if (mClaim) {
            mClaim.view.set(materialSlice, offset * 48);
          } else {
            this.device.queue.writeBuffer(
              activeMaterialBuffer,
              offset * 48,
              materialSlice,
            );
          }
        }
        offset += count;
      }
      if (tClaim) this.flushStaging(tClaim, activeTransformBuffer);
      if (mClaim) this.flushStaging(mClaim, activeMaterialBuffer);
      this.materialNeedsUpload[this.frameIndex] = false;
    }
    const t1 = performance.now();
    const commandEncoder = this.device.createCommandEncoder();

    for (const { slot, dst, bytes } of this.stagingCopies) {
      commandEncoder.copyBufferToBuffer(slot.buffer, 0, dst, 0, bytes);
    }

    const gpuSims = world.query(["GPUDrivenSimulation"]);
    const cullReadSlots = [];
    let computePassEncoder = null;
    let totalWriteTime = 0;
    let totalComputeTime = 0;

    if (gpuSims.length > 0) {
      for (let i = 0; i < gpuSims.length; i++) {
        const view = gpuSims[i];
        const arr = view.arrays["GPUDrivenSimulation"];
        for (let j = 0; j < view.len; j++) {
          const entId = view.entities[j * 2];
          const maxInstances = arr[j * 8 + 0];
          const meshId = arr[j * 8 + 1];
          const shaderType = arr[j * 8 + 2];
          const speed = arr[j * 8 + 3];
          const size = arr[j * 8 + 4];
          const gravity = arr[j * 8 + 5];
          const noiseScale = arr[j * 8 + 6];
          let state = this.gpuSimStates.get(entId);
          if (!state) {
            state = this.initGPUSim(entId, maxInstances, shaderType);
          }

          const workgroups = Math.ceil(maxInstances / SIM_WORKGROUP_SIZE);
          const dispatchX = Math.min(workgroups, MAX_DISPATCH_PER_DIM);
          const dispatchY = Math.ceil(workgroups / dispatchX);
          const rowStride = dispatchX * SIM_WORKGROUP_SIZE;

          const tWrite = performance.now();
          const simParams = new Float32Array([
            speed,
            size,
            gravity,
            noiseScale,
            this.simTime,
            dt,
            rowStride,
            0,
          ]);
          this.device.queue.writeBuffer(state.paramBuffer, 0, simParams);
          totalWriteTime += performance.now() - tWrite;

          const tComp = performance.now();
          if (!computePassEncoder) {
            computePassEncoder = commandEncoder.beginComputePass(
              this.timestampsEnabled
                ? {
                    timestampWrites: {
                      querySet: this.timestampQuerySet,
                      beginningOfPassWriteIndex: 0,
                      endOfPassWriteIndex: 1,
                    },
                  }
                : undefined,
            );
          }
          computePassEncoder.setPipeline(state.computePipeline);
          computePassEncoder.setBindGroup(0, state.computeBindGroup);
          computePassEncoder.dispatchWorkgroups(dispatchX, dispatchY);
          totalComputeTime += performance.now() - tComp;
        }
      }
      if (computePassEncoder) {
        computePassEncoder.end();
      }

      if (this.frustumCulling) {
        Renderer3D.frustumPlanes(sceneData, this.cullPlanes);
        let cullPass = null;
        const pendingCullReads = [];
        this.lastCullStats.submitted = 0;
        for (let i = 0; i < gpuSims.length; i++) {
          const view = gpuSims[i];
          const arr = view.arrays["GPUDrivenSimulation"];
          for (let j = 0; j < view.len; j++) {
            const entId = view.entities[j * 2];
            const meshId = arr[j * 8 + 1];
            const state = this.gpuSimStates.get(entId);
            const mesh = this.assets.getMesh(meshId);
            if (!state || !mesh) continue;
            this.initGPUSimCulling(state);
            const n = state.maxInstances;
            this.lastCullStats.submitted += n;

            this.indirectHost[0] = mesh.indexCount;
            this.indirectHost[1] = 0;
            this.indirectHost[2] = 0;
            this.indirectHost[3] = 0;
            this.indirectHost[4] = 0;
            this.device.queue.writeBuffer(
              state.indirectBuffer,
              0,
              this.indirectHost,
            );

            const workgroups = Math.ceil(n / SIM_WORKGROUP_SIZE);
            const dispatchX = Math.min(workgroups, MAX_DISPATCH_PER_DIM);
            const dispatchY = Math.ceil(workgroups / dispatchX);

            this.cullParamsHost.set(this.cullPlanes, 0);

            new Uint32Array(this.cullParamsHost.buffer, 96, 1)[0] = n;

            this.cullParamsHost[25] = 0.8660254;
            this.cullParamsHost[26] = dispatchX * SIM_WORKGROUP_SIZE;
            this.device.queue.writeBuffer(
              state.cullParamBuffer,
              0,
              this.cullParamsHost,
            );

            if (!cullPass) {
              cullPass = commandEncoder.beginComputePass(
                this.timestampsEnabled
                  ? {
                      timestampWrites: {
                        querySet: this.timestampQuerySet,
                        beginningOfPassWriteIndex: 4,
                        endOfPassWriteIndex: 5,
                      },
                    }
                  : undefined,
              );
            }
            cullPass.setPipeline(state.cullPipeline);
            cullPass.setBindGroup(0, state.cullBindGroup);
            cullPass.dispatchWorkgroups(dispatchX, dispatchY);
            pendingCullReads.push(state);
          }
        }
        if (cullPass) cullPass.end();

        for (const state of pendingCullReads) {
          const slot = state.cullReadBuffers.find((s) => !s.busy);
          if (!slot) continue;
          slot.busy = true;
          commandEncoder.copyBufferToBuffer(
            state.indirectBuffer,
            0,
            slot.buffer,
            0,
            8,
          );
          cullReadSlots.push(slot);
        }
      }
    }
    this.lastStats.writeBufferTimeMs = totalWriteTime;
    this.lastStats.computeRecordTimeMs = totalComputeTime;

    if (this.renderer.msaaCount > 1) {
      this.renderer.renderPassDescriptor.colorAttachments[0].view =
        this.renderer.msaaColorTextureView;
      this.renderer.renderPassDescriptor.colorAttachments[0].resolveTarget =
        this.renderer.context.getCurrentTexture().createView();
    } else {
      this.renderer.renderPassDescriptor.colorAttachments[0].view =
        this.renderer.context.getCurrentTexture().createView();
      this.renderer.renderPassDescriptor.colorAttachments[0].resolveTarget =
        undefined;
    }
    this.renderer.renderPassDescriptor.depthStencilAttachment.view =
      this.renderer.depthTextureView;
    const tRenderRec = performance.now();
    if (this.timestampsEnabled) {
      this.renderer.renderPassDescriptor.timestampWrites = {
        querySet: this.timestampQuerySet,
        beginningOfPassWriteIndex: 2,
        endOfPassWriteIndex: 3,
      };
    } else if (this.renderer.renderPassDescriptor.timestampWrites) {
      delete this.renderer.renderPassDescriptor.timestampWrites;
    }
    const pass = commandEncoder.beginRenderPass(
      this.renderer.renderPassDescriptor,
    );
    let bgBindGroup = null;
    if (this.skyPipeline) {
      bgBindGroup = this.device.createBindGroup({
        layout: this.skyPipeline.getBindGroupLayout(0),
        entries: [{ binding: 0, resource: { buffer: this.sceneBuffer3D } }],
      });
      pass.setPipeline(this.skyPipeline);
      pass.setBindGroup(0, bgBindGroup);
      pass.draw(3);
    }
    pass.setBindGroup(0, this.sceneBindGroup3D);
    if (totalInstances > 0) {
      let offset = 0;
      let lastPipeline = null;
      let lastVertexBuffer = null;
      let lastIndexBuffer = null;
      let lastTransformBuffer = null;
      let lastMaterialBuffer = null;
      const opaqueBatches = [];
      const transparentBatches = [];
      for (let i = 0; i < batchCount; i++) {
        const sId = batches[i * 5 + 2];
        const mId = batches[i * 5 + 3];
        const count = batches[i * 5 + 4];
        const matPtr = batches[i * 5 + 1];
        const matFloatArray = new Float32Array(
          world.memory.buffer,
          matPtr,
          count * 12,
        );
        const blendMode = matFloatArray[9];
        const batchInfo = { sId, mId, count, blendMode, offset };
        if (blendMode === 1.0 || blendMode === 2.0) {
          transparentBatches.push(batchInfo);
        } else {
          opaqueBatches.push(batchInfo);
        }
        offset += count;
      }
      for (const b of opaqueBatches) {
        const pipelines =
          this.materialRegistry.get(b.sId) || this.materialRegistry.get(0);
        const pipeline = pipelines ? pipelines.opaque || pipelines : null;
        const mesh = this.assets.getMesh(b.mId);
        if (mesh && pipeline) {
          if (pipeline !== lastPipeline) {
            pass.setPipeline(pipeline);
            lastPipeline = pipeline;
          }
          if (mesh.vertexBuffer !== lastVertexBuffer) {
            pass.setVertexBuffer(0, mesh.vertexBuffer);
            lastVertexBuffer = mesh.vertexBuffer;
          }
          if (mesh.indexBuffer !== lastIndexBuffer) {
            pass.setIndexBuffer(mesh.indexBuffer, "uint32");
            lastIndexBuffer = mesh.indexBuffer;
          }
          if (activeTransformBuffer !== lastTransformBuffer) {
            pass.setVertexBuffer(1, activeTransformBuffer);
            lastTransformBuffer = activeTransformBuffer;
          }
          if (activeMaterialBuffer !== lastMaterialBuffer) {
            pass.setVertexBuffer(2, activeMaterialBuffer);
            lastMaterialBuffer = activeMaterialBuffer;
          }
          pass.drawIndexed(mesh.indexCount, b.count, 0, 0, b.offset);
        }
      }
      for (const b of transparentBatches) {
        const pipelines =
          this.materialRegistry.get(b.sId) || this.materialRegistry.get(0);
        let pipeline = null;
        if (pipelines) {
          if (pipelines.opaque) {
            pipeline =
              b.blendMode === 1.0 ? pipelines.transparent : pipelines.additive;
          } else {
            pipeline = pipelines;
          }
        }
        const mesh = this.assets.getMesh(b.mId);
        if (mesh && pipeline) {
          if (pipeline !== lastPipeline) {
            pass.setPipeline(pipeline);
            lastPipeline = pipeline;
          }
          if (mesh.vertexBuffer !== lastVertexBuffer) {
            pass.setVertexBuffer(0, mesh.vertexBuffer);
            lastVertexBuffer = mesh.vertexBuffer;
          }
          if (mesh.indexBuffer !== lastIndexBuffer) {
            pass.setIndexBuffer(mesh.indexBuffer, "uint32");
            lastIndexBuffer = mesh.indexBuffer;
          }
          if (activeTransformBuffer !== lastTransformBuffer) {
            pass.setVertexBuffer(1, activeTransformBuffer);
            lastTransformBuffer = activeTransformBuffer;
          }
          if (activeMaterialBuffer !== lastMaterialBuffer) {
            pass.setVertexBuffer(2, activeMaterialBuffer);
            lastMaterialBuffer = activeMaterialBuffer;
          }
          pass.drawIndexed(mesh.indexCount, b.count, 0, 0, b.offset);
        }
      }
    }

    if (gpuSims.length > 0) {
      for (let i = 0; i < gpuSims.length; i++) {
        const view = gpuSims[i];
        const arr = view.arrays["GPUDrivenSimulation"];
        for (let j = 0; j < view.len; j++) {
          const entId = view.entities[j * 2];
          const meshId = arr[j * 8 + 1];
          const state = this.gpuSimStates.get(entId);
          const mesh = this.assets.getMesh(meshId);
          if (state && mesh) {
            const culled = this.frustumCulling && state.cullPipeline;
            pass.setPipeline(state.renderPipeline);
            pass.setBindGroup(0, this.sceneBindGroup3D);
            pass.setVertexBuffer(0, mesh.vertexBuffer);
            pass.setVertexBuffer(
              1,
              culled ? state.visibleBuffer : state.particleBuffer,
            );
            pass.setVertexBuffer(
              2,
              culled ? state.origIndexBuffer : state.identityIndexBuffer,
            );
            if (mesh.indexBuffer) {
              pass.setIndexBuffer(mesh.indexBuffer, "uint32");
              if (culled) {

                pass.drawIndexedIndirect(state.indirectBuffer, 0);
              } else {
                pass.drawIndexed(mesh.indexCount, state.maxInstances, 0, 0, 0);
              }
            } else {
              pass.draw(mesh.indexCount, state.maxInstances, 0, 0);
            }
          }
        }
      }
    }

    if (this.editorMode && bgBindGroup) {
      pass.setPipeline(this.gridPipeline);
      pass.setBindGroup(0, bgBindGroup);
      pass.draw(3);
      if (this.selectedEntity !== null) {
        const ptr = world.wasm.get_component_ptr(
          this.selectedEntity,
          "GlobalTransform",
        );
        if (ptr !== 0) {
          const gt = new Float32Array(world.memory.buffer, ptr, 16);
          this.device.queue.writeBuffer(
            this.gizmoBuffer,
            0,
            new Float32Array([gt[12], gt[13], gt[14], 1.0]),
          );
          pass.setPipeline(this.gizmoPipeline);
          pass.setBindGroup(0, bgBindGroup);
          pass.setBindGroup(1, this.gizmoBindGroup);
          pass.draw(6);
        }
      }
    }
    pass.end();
    this.lastStats.renderRecordTimeMs = performance.now() - tRenderRec;

    let readSlot = null;
    if (this.timestampsEnabled) {
      readSlot = this.timestampReadBuffers.find((s) => !s.busy);
      if (readSlot) {
        readSlot.busy = true;
        commandEncoder.resolveQuerySet(
          this.timestampQuerySet,
          0,
          6,
          this.timestampResolveBuffer,
          0,
        );
        commandEncoder.copyBufferToBuffer(
          this.timestampResolveBuffer,
          0,
          readSlot.buffer,
          0,
          48,
        );
      }
    }

    const hadCompute = computePassEncoder !== null;
    const hadCull = cullReadSlots.length > 0;

    const tSubmit = performance.now();
    this.device.queue.submit([commandEncoder.finish()]);

    this.remapStagingSlots();
    this.device.queue.onSubmittedWorkDone().then(() => {
      this.lastStats.gpuExecutionTimeMs = performance.now() - tSubmit;
    });

    for (const slot of cullReadSlots) {
      slot.buffer.mapAsync(GPUMapMode.READ).then(
        () => {
          this.lastCullStats.drawn = new Uint32Array(
            slot.buffer.getMappedRange().slice(0),
          )[1];
          slot.buffer.unmap();
          slot.busy = false;
        },
        () => {
          slot.busy = false;
        },
      );
    }

    if (readSlot) {
      readSlot.buffer.mapAsync(GPUMapMode.READ).then(
        () => {
          const t = new BigInt64Array(readSlot.buffer.getMappedRange().slice(0));
          readSlot.buffer.unmap();
          readSlot.busy = false;

          const span = (a, b) => {
            const d = Number(t[b] - t[a]) / 1e6;
            return d >= 0 && d < 10000 ? d : null;
          };
          if (hadCompute) {
            const c = span(0, 1);
            if (c !== null) this.lastStats.gpuComputePassMs = c;
          } else {
            this.lastStats.gpuComputePassMs = 0;
          }
          const r = span(2, 3);
          if (r !== null) this.lastStats.gpuRenderPassMs = r;
          if (hadCull) {
            const cu = span(4, 5);
            if (cu !== null) this.lastStats.gpuCullPassMs = cu;
          } else {
            this.lastStats.gpuCullPassMs = 0;
          }
        },
        () => {
          readSlot.busy = false;
        },
      );
    }

    const t2 = performance.now();
    this.lastStats.batches = batchCount;
    this.lastStats.instances = totalInstances;
    this.lastStats.uploadTimeMs = t1 - t0;
    this.lastStats.passTimeMs = t2 - t1;
    this.frameIndex = (this.frameIndex + 1) % 3;
  }
}
