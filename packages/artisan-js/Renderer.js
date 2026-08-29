import { AssetManager } from "./Assets.js";
import { Renderer2D } from "./Renderer2D.js";
import { Renderer3D } from "./Renderer3D.js";
import { MeshSyncer } from "./MeshSyncer.js";

async function requestDeviceWithMaxBuffers(adapter) {
  const wanted = [
    "maxBufferSize",
    "maxStorageBufferBindingSize",
    "maxUniformBufferBindingSize",
    "maxVertexBufferArrayStride",
  ];
  const requiredLimits = {};
  for (const key of wanted) {
    const v = adapter.limits[key];
    if (typeof v === "number") requiredLimits[key] = v;
  }

  const optionalFeatures = ["timestamp-query"].filter((f) =>
    adapter.features.has(f),
  );
  try {
    return await adapter.requestDevice({
      requiredLimits,
      requiredFeatures: optionalFeatures,
    });
  } catch (e) {
    console.warn("[artisan] adapter limits refused, using defaults:", e);
    return await adapter.requestDevice();
  }
}

export class WebGPURenderer {

  constructor(canvas, options = {}) {
    this.canvas = canvas;
    this.device = null;
    this.context = null;
    this.assets = null;
    this.gpuFormat = null;
    this.renderPassDescriptor = null;

    this.msaaCount = [1, 2, 4].includes(options.msaa) ? options.msaa : 4;
    this.msaaColorTexture = null;
    this.msaaColorTextureView = null;
    this.depthTexture = null;
    this.depthTextureView = null;

    this.renderer2D = new Renderer2D(this);
    this.renderer3D = new Renderer3D(this);
    this.meshSyncer = null;
  }

  registerGPUSimShader(typeId, computeWgsl, renderWgsl) {
    this.renderer3D.registerGPUSimShader(typeId, computeWgsl, renderWgsl);
  }

  async init() {
    if (!navigator.gpu) throw new Error("WebGPU not supported");

    const adapter = await navigator.gpu.requestAdapter({
      powerPreference: "high-performance",
    });

    this.device = await requestDeviceWithMaxBuffers(adapter);
    this.context = this.canvas.getContext("webgpu");
    this.gpuFormat = navigator.gpu.getPreferredCanvasFormat();

    const config = {
      device: this.device,
      format: this.gpuFormat,
      alphaMode: "premultiplied",
    };

    const supportedModes = adapter.features;
    if (supportedModes.has("immediate")) {
      config.presentMode = "immediate";
    } else if (supportedModes.has("mailbox")) {
      config.presentMode = "mailbox";
    }

    try {
      this.context.configure(config);
    } catch (_) {
      config.presentMode = "fifo";
      this.context.configure(config);
    }

    this.assets = new AssetManager(this.device);
    this.meshSyncer = new MeshSyncer(this.device, this.assets);

    this.renderPassDescriptor = {
      colorAttachments: [
        {
          view: undefined,
          resolveTarget: undefined,
          clearValue: { r: 0.08, g: 0.1, b: 0.12, a: 1.0 },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
      depthStencilAttachment: {
        view: undefined,
        depthClearValue: 1.0,
        depthLoadOp: "clear",
        depthStoreOp: "store",
      },
    };

    this.deviceLost = null;
    this.device.lost?.then((info) => {
      this.deviceLost = info;
      console.error(
        `[artisan] WebGPU device lost (${info.reason}): ${info.message}`,
      );
    });

    this.resize();

    const observer = new ResizeObserver(() => {
      this.resize();
    });
    observer.observe(this.canvas);

    document.addEventListener("fullscreenchange", () => this.resize());
    window.addEventListener("resize", () => this.resize());

    this.renderer2D.init();
    this.renderer3D.init();
  }

  syncCanvasSize() {
    if (this.deviceLost) return false;
    const w = this.targetSize();
    if (!w) return false;
    if (
      w[0] === this.canvas.width &&
      w[1] === this.canvas.height &&
      (this.msaaCount === 1 || this.msaaColorTextureView) &&
      this.depthTextureView
    ) {
      return true;
    }
    this.resize();
    return !!this.depthTextureView;
  }

  targetSize() {
    const w = Math.floor(this.canvas.clientWidth || window.innerWidth || 0);
    const h = Math.floor(this.canvas.clientHeight || window.innerHeight || 0);

    if (w < 1 || h < 1) return null;

    const max = this.device?.limits?.maxTextureDimension2D || 8192;
    return [Math.min(w, max), Math.min(h, max)];
  }

  resize() {
    if (this.deviceLost) return;
    const size = this.targetSize();
    if (!size) return;
    const [w, h] = size;

    try {
      this.canvas.width = w;
      this.canvas.height = h;

      this.msaaColorTexture?.destroy();
      this.depthTexture?.destroy();
      this.msaaColorTexture = null;
      this.msaaColorTextureView = null;
      this.depthTexture = null;
      this.depthTextureView = null;

      if (this.msaaCount > 1) {
        this.msaaColorTexture = this.device.createTexture({
          size: [w, h],
          sampleCount: this.msaaCount,
          format: this.gpuFormat,
          usage: GPUTextureUsage.RENDER_ATTACHMENT,
        });
        this.msaaColorTextureView = this.msaaColorTexture.createView();
      }

      this.depthTexture = this.device.createTexture({
        size: [w, h],
        sampleCount: this.msaaCount,
        format: "depth24plus",
        usage: GPUTextureUsage.RENDER_ATTACHMENT,
      });
      this.depthTextureView = this.depthTexture.createView();
    } catch (err) {

      console.error("[artisan] resize failed, skipping this frame:", err);
      this.msaaColorTextureView = null;
      this.depthTextureView = null;
    }
  }

  syncCameras(world) {
    const aspect = this.canvas.width / this.canvas.height || 16 / 9;

    const cams3d = world.query(["Camera3D"]);
    for (let i = 0; i < cams3d.length; i++) {
      const view = cams3d[i];
      const arr = view.arrays["Camera3D"];
      for (let j = 0; j < view.len; j++) {
        arr[j * 40 + 1] = aspect;
      }
    }

    const cams2d = world.query(["Camera"]);
    for (let i = 0; i < cams2d.length; i++) {
      const view = cams2d[i];
      const arr = view.arrays["Camera"];
      for (let j = 0; j < view.len; j++) {
        arr[j * 2 + 0] = arr[j * 2 + 0];
      }
    }
  }

  setClearColor(r, g, b, a) {
    this.renderPassDescriptor.colorAttachments[0].clearValue = { r, g, b, a };
  }

  createQuadMesh() {
    return this.renderer2D.createQuadMesh();
  }

  createDataTexture(width, height, data) {
    return this.assets.createDataTexture(width, height, data);
  }

  screenToWorld(screenX, screenY, cameraEntityView) {
    return this.renderer2D.screenToWorld(screenX, screenY, cameraEntityView);
  }

  createShader(id, wgsl) {
    this.renderer3D.createShader(id, wgsl);
  }

  render(world) {
    if (!this.syncCanvasSize()) return;
    this.syncCameras(world);
    this.meshSyncer.sync(world);
    this.renderer2D.render(world);
  }

  render3D(world, dt) {
    if (!this.syncCanvasSize()) return;
    this.syncCameras(world);
    this.meshSyncer.sync(world);
    this.renderer3D.render(world, dt);
  }
}
