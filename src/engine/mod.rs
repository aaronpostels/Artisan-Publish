use crate::ecs::{IntoSystemConfig, SystemNode, System, World, Query, Res, ResMut, Entity, With};
use crate::ecs::component::Resource;
use crate::engine::component::{Transform2D, Velocity, Collider, BoxCollider2D, CapsuleCollider2D, PathFollower, Collider3D, SpriteMaterial, SpriteAnimation, Camera3D, GlobalTransform, AABB, Visibility, Transform, AngularVelocity, FlyCamera};
use rayon::prelude::*;
use glam::{Mat4, Vec3, Quat};

pub mod component;
pub mod math;
pub mod spatial;
pub mod spatial_3d;
pub mod transform;
pub mod mesh;
pub mod input;
pub mod logger;
pub mod clock;

use input::{Input, KeyCode, MouseButton};

#[derive(Default)] pub struct Time { pub delta_seconds: f32, pub elapsed_seconds: f32, }
pub trait Plugin: Send + Sync { fn build(&self, app: &mut App); }
pub trait PluginGroup { fn build(&self, app: &mut App); }
pub trait AnyState: Resource + PartialEq + Copy + Send + Sync {}
impl<T: Resource + PartialEq + Copy + Send + Sync> AnyState for T {}
pub struct State<S: AnyState>(pub S);
pub fn in_state<S: AnyState>(state: S) -> Box<dyn Fn(&World) -> bool + Send + Sync> {
    Box::new(move |w| w.get_resource::<State<S>>().map_or(false, |s| s.0 == state))
}
pub struct FixedTime { pub step: f32, pub accumulator: f32 }
impl Default for FixedTime { fn default() -> Self { Self { step: 1.0/60.0, accumulator: 0.0 } } }

pub fn sys_integrate_velocity(mut q: Query<'_, (&mut Transform2D, &Velocity)>, time: Res<Time>) {
    let dt = time.delta_seconds;
    q.par_for_each(|(pos, vel)| {
        pos.x += vel.x * dt;
        pos.y += vel.y * dt;
    });
}

pub fn sys_update_path_followers(
    mut q: Query<'_, (&mut Transform2D, &mut PathFollower)>,
    world: &World,
    time: Res<Time>
) {
    let dt = time.delta_seconds;
    let t_id = world.get_component_id::<Transform2D>();
    if let Some(tid) = t_id {
        q.par_for_each(|(t, follower)| {
            if follower.active == 0.0 || follower.target_id == u32::MAX as f32 { return; }
            let target_ent = Entity {
                id: follower.target_id as u32,
                generation: follower.target_gen as u32,
            };
            if let Some(target_t) = world.get_component_by_id::<Transform2D>(target_ent, tid) {
                let dx = target_t.x - t.x;
                let dy = target_t.y - t.y;
                let dist = (dx*dx + dy*dy).sqrt();
                let step = follower.speed * dt;
                if dist <= step {
                    t.x = target_t.x;
                    t.y = target_t.y;
                    follower.progress = 1.0;
                } else if dist > 0.0 {
                    t.x += (dx / dist) * step;
                    t.y += (dy / dist) * step;
                    follower.progress = step / dist;
                }
            }
        });
    }
}

pub fn sys_apply_angular_velocity(mut q: Query<'_, (&mut Transform, &AngularVelocity)>, time: Res<Time>) {
    let dt = time.delta_seconds;
    q.par_for_each(|(t, av)| {
        let axis = Vec3::new(av.x, av.y, av.z);
        let angle = axis.length();
        if angle > 1e-6 {
            let rotation = Quat::from_axis_angle(axis / angle, angle * dt);
            let mut q = Quat::from_array(t.rotation);
            q = (rotation * q).normalize();
            t.rotation = q.to_array();
        }
    });
}

pub fn sys_sprite_animate(mut q: Query<'_, (&mut SpriteMaterial, &mut SpriteAnimation)>, time: Res<Time>) {
    let dt = time.delta_seconds;
    q.par_for_each(|(mat, anim)| {
        anim.timer += dt;
        let frame_time = 1.0 / anim.fps;
        if anim.timer >= frame_time {
            anim.timer -= frame_time;
            anim.current_frame = (anim.current_frame + 1.0) % anim.frame_count;
            mat.uv_x = anim.current_frame * anim.width_per_frame;
        }
    });
}

pub fn sys_fly_camera(mut q: Query<'_, (&mut Transform, &mut FlyCamera)>, input: Res<Input>, time: Res<Time>) {
    let dt = time.delta_seconds;
    q.par_for_each(|(t, cam)| {
        if cam.active == 0 { return; }

        if input.mouse_pressed(MouseButton::Right) {
            cam.yaw -= input.mouse_dx * cam.sensitivity;
            cam.pitch -= input.mouse_dy * cam.sensitivity;
            cam.pitch = cam.pitch.clamp(-1.57, 1.57);
        }

        let rot = Quat::from_euler(glam::EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
        t.rotation = rot.to_array();

        let forward = rot * Vec3::NEG_Z;
        let right = rot * Vec3::X;
        let up = Vec3::Y;

        let mut mv = Vec3::ZERO;
        if input.pressed(KeyCode::W) { mv += forward; }
        if input.pressed(KeyCode::S) { mv -= forward; }
        if input.pressed(KeyCode::A) { mv -= right; }
        if input.pressed(KeyCode::D) { mv += right; }
        if input.pressed(KeyCode::E) { mv += up; }
        if input.pressed(KeyCode::Q) { mv -= up; }

        if mv.length_squared() > 0.0 {
            let mv = mv.normalize() * cam.speed * dt;
            t.translation[0] += mv.x;
            t.translation[1] += mv.y;
            t.translation[2] += mv.z;
        }
    });
}

pub fn sys_update_camera_matrices(mut q: Query<'_, (&mut Camera3D, &GlobalTransform)>) {
    q.par_for_each(|(cam, gt)| {
        let view = gt.matrix.inverse();
        #[allow(deprecated)]
        let proj = glam::Mat4::perspective_rh(cam.fov, cam.aspect, cam.near, cam.far);
        let vp = proj * view;
        cam.view_proj = vp.to_cols_array();
        cam.inv_view_proj = vp.inverse().to_cols_array();
        cam.camera_pos = gt.matrix.w_axis.truncate().to_array();
    });
}

#[derive(Default)]
pub struct VisibilityGen(pub std::sync::atomic::AtomicU64);

impl VisibilityGen {
    #[inline]
    pub fn get(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[inline]
    pub fn bump(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

pub fn sys_frustum_culling(
    vis_gen: Res<'_, VisibilityGen>,
    mut q_cam: Query<'_, &Camera3D>,
    mut q_visible: Query<'_, (&GlobalTransform, &AABB, &mut Visibility)>
) {
    let cam = match q_cam.iter().next() {
        Some(c) => c,
        None => return,
    };

    let vp = Mat4::from_cols_array(&cam.view_proj);
    let frustum = crate::engine::math::Frustum::from_matrix(vp);

    q_visible.par_for_each(|(gt, aabb, vis)| {
        let min = Vec3::from_array(aabb.min);
        let max = Vec3::from_array(aabb.max);

        let local_center = (min + max) * 0.5;
        let extents = max - min;

        let local_radius = (extents.x * extents.x + extents.y * extents.y + extents.z * extents.z).sqrt() * 0.5;

        let scale_sq = gt.matrix.x_axis.length_squared()
            .max(gt.matrix.y_axis.length_squared())
            .max(gt.matrix.z_axis.length_squared());

        let world_radius = local_radius * scale_sq.sqrt();
        let world_center = gt.matrix.transform_point3(local_center);

        let mut visible = 1;
        for plane in &frustum.planes {
            if plane.dot_point(world_center) < -world_radius {
                visible = 0;
                break;
            }
        }

        if vis.visible != visible {
            vis.visible = visible;
            vis_gen.bump();
        }
    });
}

pub fn sys_update_spatial_grid(
    mut grid: ResMut<crate::engine::spatial::SpatialGrid>,
    mut q_circ: Query<'_, (&Transform2D, Entity), With<Collider>>,
    mut q_box: Query<'_, (&Transform2D, Entity), With<BoxCollider2D>>,
    mut q_cap: Query<'_, (&Transform2D, Entity), With<CapsuleCollider2D>>
) {
    grid.clear();
    q_circ.for_each(|(pos, ent)| {
        grid.insert(pos.x, pos.y, ent);
    });
    q_box.for_each(|(pos, ent)| {
        grid.insert(pos.x, pos.y, ent);
    });
    q_cap.for_each(|(pos, ent)| {
        grid.insert(pos.x, pos.y, ent);
    });
}

pub fn sys_update_spatial_grid_3d(mut grid: ResMut<crate::engine::spatial_3d::SpatialGrid3D>, mut q: Query<'_, (&Transform, Entity), With<Collider3D>>) {
    grid.clear();
    q.for_each(|(pos, ent)| {
        grid.insert(pos.translation[0], pos.translation[1], pos.translation[2], ent);
    });
}

#[derive(Clone, Copy, Default)]
pub struct SystemRun {
    pub stage: u32,
    pub index_in_stage: u32,
    pub thread: u32,

    pub start_ms: f64,
    pub end_ms: f64,
}

pub struct Schedule {
    pub unbuilt: Vec<SystemNode>,
    pub stages: Vec<Vec<Box<dyn System>>>,

    pub stage_names: Vec<Vec<&'static str>>,

    pub parallel: bool,

    pub instrument: bool,

    pub trace: Vec<SystemRun>,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            unbuilt: Vec::new(),
            stages: Vec::new(),
            stage_names: Vec::new(),
            parallel: true,
            instrument: false,
            trace: Vec::new(),
        }
    }
}

impl Schedule {
    pub fn add_system(&mut self, node: SystemNode) { self.unbuilt.push(node); }

    pub fn describe(&self, world: &World) -> String {
        let name_of = |tid: &std::any::TypeId| -> String {
            match world.get_component_id_by_type_id(*tid) {
                Some(cid) => world
                    .schemas
                    .iter()
                    .find(|s| s.id == cid)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| format!("#{cid}")),

                None => "-".to_string(),
            }
        };
        let sorted = |set: &std::collections::HashSet<std::any::TypeId>| {
            let mut v: Vec<String> = set.iter().map(&name_of).collect();
            v.sort();
            v.dedup();
            v
        };

        let mut out = String::from("{\"stages\":[");
        for (si, stage) in self.stages.iter().enumerate() {
            if si > 0 { out.push(','); }
            out.push('[');
            for (i, sys) in stage.iter().enumerate() {
                if i > 0 { out.push(','); }
                let access = sys.access();
                let name = self
                    .stage_names
                    .get(si)
                    .and_then(|n| n.get(i))
                    .copied()
                    .unwrap_or("system");
                let short = name.rsplit("::").next().unwrap_or(name);
                out.push_str(&format!(
                    "{{\"name\":{},\"reads\":{},\"writes\":{}}}",
                    json_str(short),
                    json_arr(&sorted(&access.reads)),
                    json_arr(&sorted(&access.writes)),
                ));
            }
            out.push(']');
        }
        out.push_str("]}");
        out
    }
    pub fn build(&mut self) {
        if self.unbuilt.is_empty() { return; }
        let mut nodes = std::mem::take(&mut self.unbuilt);
        let n = nodes.len();
        let mut adj = vec![vec![]; n];
        let mut in_degree = vec![0; n];
        for i in 0..n {
            for j in 0..n {
                if i == j { continue; }
                let mut i_before_j = false;
                for b in &nodes[i].before { if nodes[j].labels.contains(b) { i_before_j = true; } }
                for a in &nodes[j].after { if nodes[i].labels.contains(a) { i_before_j = true; } }
                if i_before_j { adj[i].push(j); in_degree[j] += 1; }
            }
        }
        let mut queue = std::collections::VecDeque::new();
        for i in 0..n { if in_degree[i] == 0 { queue.push_back(i); } }
        let mut sorted = Vec::new();
        while let Some(u) = queue.pop_front() {
            sorted.push(u);
            for &v in &adj[u] { in_degree[v] -= 1; if in_degree[v] == 0 { queue.push_back(v); } }
        }
        self.stages.clear();
        self.stage_names.clear();
        struct DummySys;
        impl System for DummySys { fn run(&mut self, _w: &World) {} fn access(&self) -> crate::ecs::Access { crate::ecs::Access::default() } }
        for &idx in &sorted {
            let node = std::mem::replace(&mut nodes[idx], SystemNode { system: Box::new(DummySys), name: "", labels: vec![], before: vec![], after: vec![] });
            let name = node.name;
            let mut sys_opt = Some(node.system);
            if let Some(last_stage) = self.stages.last_mut() {
                let access = sys_opt.as_ref().unwrap().access();
                let mut compatible = true;
                for s in last_stage.iter() { if !access.is_compatible_with(&s.access()) { compatible = false; break; } }
                if compatible {
                    last_stage.push(sys_opt.take().unwrap());
                    self.stage_names.last_mut().unwrap().push(name);
                }
            }
            if let Some(s) = sys_opt { self.stages.push(vec![s]); self.stage_names.push(vec![name]); }
        }
    }
    pub fn run(&mut self, world: &mut World) {
        self.build();
        if self.instrument {
            self.run_instrumented(world);
            return;
        }
        if !self.parallel {
            for stage in &mut self.stages { for system in stage.iter_mut() { system.run(world); } world.apply_commands(); }
            return;
        }
        for stage in &mut self.stages { stage.par_iter_mut().for_each(|system| { system.run(world); }); world.apply_commands(); }
    }

    fn run_instrumented(&mut self, world: &mut World) {
        self.trace.clear();
        let parallel = self.parallel;
        for (si, stage) in self.stages.iter_mut().enumerate() {

            let runs: Vec<SystemRun> = if parallel {
                stage
                    .par_iter_mut()
                    .enumerate()
                    .map(|(i, system)| {
                        let start_ms = clock::now_ms();
                        system.run(world);
                        SystemRun {
                            stage: si as u32,
                            index_in_stage: i as u32,
                            thread: rayon::current_thread_index().unwrap_or(0) as u32,
                            start_ms,
                            end_ms: clock::now_ms(),
                        }
                    })
                    .collect()
            } else {
                stage
                    .iter_mut()
                    .enumerate()
                    .map(|(i, system)| {
                        let start_ms = clock::now_ms();
                        system.run(world);
                        SystemRun {
                            stage: si as u32,
                            index_in_stage: i as u32,
                            thread: 0,
                            start_ms,
                            end_ms: clock::now_ms(),
                        }
                    })
                    .collect()
            };
            self.trace.extend_from_slice(&runs);
            world.apply_commands();
        }
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_arr(v: &[String]) -> String {
    let mut out = String::from("[");
    for (i, s) in v.iter().enumerate() {
        if i > 0 { out.push(','); }
        out.push_str(&json_str(s));
    }
    out.push(']');
    out
}
pub struct App { pub world: World, pub update_schedule: Schedule, pub fixed_schedule: Schedule, pub render_schedule: Schedule, pub event_flushers: Vec<fn(&mut World)>, }
impl Default for App { fn default() -> Self { Self::new() } }
impl App {
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(FixedTime::default());
        Self { world, update_schedule: Schedule::default(), fixed_schedule: Schedule::default(), render_schedule: Schedule::default(), event_flushers: Vec::new() }
    }
    pub fn add_plugin<T: Plugin>(&mut self, plugin: T) -> &mut Self { plugin.build(self); self }
    pub fn add_plugins<T: PluginGroup>(&mut self, group: T) -> &mut Self { group.build(self); self }
    pub fn add_event<T: Resource>(&mut self) -> &mut Self { self.init_resource::<crate::ecs::Events<T>>(); self.event_flushers.push(|w| { if let Some(evs) = w.get_resource_mut::<crate::ecs::Events<T>>() { evs.update(); } }); self }
    pub fn insert_resource<T: Resource>(&mut self, resource: T) -> &mut Self { self.world.insert_resource(resource); self.world.apply_commands(); self }
    pub fn init_resource<T: Resource + Default>(&mut self) -> &mut Self { self.world.insert_resource(T::default()); self }
    pub fn add_system<M>(&mut self, system: impl IntoSystemConfig<M>) -> &mut Self { let node = system.into_config(&mut self.world); self.update_schedule.add_system(node); self }
    pub fn add_fixed_system<M>(&mut self, system: impl IntoSystemConfig<M>) -> &mut Self { let node = system.into_config(&mut self.world); self.fixed_schedule.add_system(node); self }

    pub fn add_render_system<M>(&mut self, system: impl IntoSystemConfig<M>) -> &mut Self { let node = system.into_config(&mut self.world); self.render_schedule.add_system(node); self }
    pub fn add_systems_to_state<S: AnyState, M>(&mut self, state: S, system: impl IntoSystemConfig<M>) -> &mut Self { self.add_system(system.run_if(in_state(state))) }
    pub fn update(&mut self) {
        let dt = self.world.get_resource::<Time>().map(|t| t.delta_seconds).unwrap_or(0.016);
        let mut steps_to_run = 0;
        if let Some(ft) = self.world.get_resource_mut::<FixedTime>() {
            ft.accumulator += dt;
            let step = ft.step;
            if ft.accumulator >= step {
                steps_to_run = (ft.accumulator / step) as u32;
                ft.accumulator -= steps_to_run as f32 * step;
            }
        }
        for _ in 0..steps_to_run {
            self.world.current_tick += 1;
            self.fixed_schedule.run(&mut self.world);
        }
        self.world.current_tick += 1;
        self.update_schedule.run(&mut self.world);
        for f in &self.event_flushers { f(&mut self.world); }
    }

    pub fn render_update(&mut self) {
        self.render_schedule.run(&mut self.world);
    }
}
