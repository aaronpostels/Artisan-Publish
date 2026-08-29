use crate::ecs::{Children, Parent, Query, Without, World, Entity, With};
use crate::ecs::commands::Commands;
use crate::engine::component::{GlobalTransform, GlobalTransform2D, Transform, Transform2D, GPUInstanceTransform, Billboard, Camera3D};
use glam::{Mat4, Quat, Vec3};

#[inline(always)]
fn transform_to_mat4(t: &Transform) -> Mat4 {
    let rot = Quat::from_array(t.rotation);
    Mat4::from_scale_rotation_translation(
        Vec3::from_array(t.scale),
        rot,
        Vec3::from_array(t.translation),
    )
}

pub fn sys_auto_add_gpu_transforms(
    mut q: Query<Entity, (With<GlobalTransform>, Without<GPUInstanceTransform>)>,
    mut cmds: Commands,
) {
    q.for_each(|ent| {
        cmds.insert(ent, GPUInstanceTransform::default());
    });
}

pub fn sys_update_billboards(
    mut q_billboards: Query<'_, (&mut Transform, &Billboard)>,
    mut q_camera: Query<'_, &Transform, With<Camera3D>>,
) {
    if let Some(cam_transform) = q_camera.iter().next() {
        let rot = cam_transform.rotation;
        q_billboards.par_for_each(|(t, _b)| {
            t.rotation = rot;
        });
    }
}

pub fn update_global_transforms(
    mut q_all_roots: Query<(&Transform, Option<&mut GlobalTransform>, Option<&mut GPUInstanceTransform>), Without<Parent>>,
    mut q_branches: Query<(&GlobalTransform, &Children), Without<Parent>>,
    mut q_children_gpu: Query<(&GlobalTransform, Option<&mut GPUInstanceTransform>), With<Parent>>,
    world: &World,
) {
    q_all_roots.par_for_each(|(t, gt, gpu_t)| {
        if let Some(gt) = gt {
            gt.matrix = transform_to_mat4(t);
        }
        if let Some(gpu_t) = gpu_t {
            gpu_t.translation = t.translation;
            let q = t.rotation;
            gpu_t.rotation = [
                (q[0] * 32767.0) as i16,
                (q[1] * 32767.0) as i16,
                (q[2] * 32767.0) as i16,
                (q[3] * 32767.0) as i16,
            ];
            gpu_t.scale = t.scale[0];
        }
    });

    q_branches.par_for_each(|(root_gt, children)| {
        let records_ptr = world.records.as_ptr();
        let archetypes_ptr = world.archetypes.as_ptr();

        let mut stack_entities = [0usize; 64];
        let mut stack_matrices = [Mat4::IDENTITY; 64];
        let mut stack_depths = [0u8; 64];
        let mut stack_ptr = 0;

        for &child in &children.0 {
            if stack_ptr < 64 {
                stack_entities[stack_ptr] = child.id as usize;
                stack_matrices[stack_ptr] = root_gt.matrix;
                stack_depths[stack_ptr] = 1;
                stack_ptr += 1;
            }
        }

        let t_id = world.get_component_id::<Transform>().unwrap();
        let gt_id = world.get_component_id::<GlobalTransform>().unwrap();
        let c_id = world.get_component_id::<Children>();

        let mut last_arch_id = u32::MAX;
        let mut cached_t_ptr = std::ptr::null::<Transform>();
        let mut cached_gt_ptr = std::ptr::null_mut::<GlobalTransform>();
        let mut cached_c_ptr = std::ptr::null::<Children>();
        let mut cached_has_c = false;

        while stack_ptr > 0 {
            stack_ptr -= 1;
            let id = stack_entities[stack_ptr];
            let parent_mat = stack_matrices[stack_ptr];
            let depth = stack_depths[stack_ptr];

            let rec = unsafe { &*records_ptr.add(id) };
            if rec.archetype_id == u32::MAX { continue; }

            if rec.archetype_id != last_arch_id {
                last_arch_id = rec.archetype_id;
                let arch = unsafe { &*archetypes_ptr.add(rec.archetype_id as usize) };
                let t_col_idx = arch.component_to_column[t_id];
                let gt_col_idx = arch.component_to_column[gt_id];

                if t_col_idx != u32::MAX && gt_col_idx != u32::MAX {
                    cached_t_ptr = unsafe { (*arch.columns[t_col_idx as usize].get()).data.as_ptr::<Transform>() };
                    cached_gt_ptr = unsafe { (*arch.columns[gt_col_idx as usize].get()).data.as_mut_ptr::<GlobalTransform>() };

                    if let Some(children_id) = c_id {
                        let c_col_idx = arch.component_to_column[children_id];
                        if c_col_idx != u32::MAX {
                            cached_c_ptr = unsafe { (*arch.columns[c_col_idx as usize].get()).data.as_ptr::<Children>() };
                            cached_has_c = true;
                        } else { cached_has_c = false; }
                    } else { cached_has_c = false; }
                } else { cached_t_ptr = std::ptr::null(); }
            }

            if cached_t_ptr.is_null() { continue; }

            let row = rec.row as usize;
            let t = unsafe { &*cached_t_ptr.add(row) };
            let gt = unsafe { &mut *cached_gt_ptr.add(row) };

            let global_mat = parent_mat * transform_to_mat4(t);
            gt.matrix = global_mat;

            if cached_has_c && depth < 32 {
                let children_comp = unsafe { &*cached_c_ptr.add(row) };
                for &child in &children_comp.0 {
                    if stack_ptr < 64 {
                        stack_entities[stack_ptr] = child.id as usize;
                        stack_matrices[stack_ptr] = global_mat;
                        stack_depths[stack_ptr] = depth + 1;
                        stack_ptr += 1;
                    }
                }
            }
        }
    });

    q_children_gpu.par_for_each(|(gt, gpu_t)| {
        if let Some(gpu_t) = gpu_t {
            let (scale, rotation, translation): (glam::Vec3, glam::Quat, glam::Vec3) = gt.matrix.to_scale_rotation_translation();
            gpu_t.translation = translation.to_array();
            let q = rotation.to_array();
            gpu_t.rotation = [
                (q[0] * 32767.0) as i16,
                (q[1] * 32767.0) as i16,
                (q[2] * 32767.0) as i16,
                (q[3] * 32767.0) as i16,
            ];
            gpu_t.scale = scale.x;
        }
    });
}

pub fn update_global_transforms_2d(
    mut q_all_roots: Query<(&Transform2D, &mut GlobalTransform2D), Without<Parent>>,
    mut q_branches: Query<(&GlobalTransform2D, &Children), Without<Parent>>,
    world: &World,
) {
    q_all_roots.par_for_each(|(t, gt)| {
        gt.x = t.x; gt.y = t.y; gt.z = t.z; gt.rotation = t.rotation; gt.scale = t.scale;
    });

    q_branches.par_for_each(|(root_gt, children)| {
        let records_ptr = world.records.as_ptr();
        let archetypes_ptr = world.archetypes.as_ptr();

        let mut stack_entities = [0usize; 64];
        let mut stack_gts = [GlobalTransform2D::default(); 64];
        let mut stack_depths = [0u8; 64];
        let mut stack_ptr = 0;

        for &child in &children.0 {
            if stack_ptr < 64 {
                stack_entities[stack_ptr] = child.id as usize;
                stack_gts[stack_ptr] = *root_gt;
                stack_depths[stack_ptr] = 1;
                stack_ptr += 1;
            }
        }

        let t_id = world.get_component_id::<Transform2D>().unwrap();
        let gt_id = world.get_component_id::<GlobalTransform2D>().unwrap();
        let c_id = world.get_component_id::<Children>();

        let mut last_arch_id = u32::MAX;
        let mut cached_t_ptr = std::ptr::null::<Transform2D>();
        let mut cached_gt_ptr = std::ptr::null_mut::<GlobalTransform2D>();
        let mut cached_c_ptr = std::ptr::null::<Children>();
        let mut cached_has_c = false;

        while stack_ptr > 0 {
            stack_ptr -= 1;
            let id = stack_entities[stack_ptr];
            let parent_gt = stack_gts[stack_ptr];
            let depth = stack_depths[stack_ptr];

            let rec = unsafe { &*records_ptr.add(id) };
            if rec.archetype_id == u32::MAX { continue; }

            if rec.archetype_id != last_arch_id {
                last_arch_id = rec.archetype_id;
                let arch = unsafe { &*archetypes_ptr.add(rec.archetype_id as usize) };
                let t_col_idx = arch.component_to_column[t_id];
                let gt_col_idx = arch.component_to_column[gt_id];

                if t_col_idx != u32::MAX && gt_col_idx != u32::MAX {
                    cached_t_ptr = unsafe { (*arch.columns[t_col_idx as usize].get()).data.as_ptr::<Transform2D>() };
                    cached_gt_ptr = unsafe { (*arch.columns[gt_col_idx as usize].get()).data.as_mut_ptr::<GlobalTransform2D>() };
                    if let Some(children_id) = c_id {
                        let c_col_idx = arch.component_to_column[children_id];
                        if c_col_idx != u32::MAX {
                            cached_c_ptr = unsafe { (*arch.columns[c_col_idx as usize].get()).data.as_ptr::<Children>() };
                            cached_has_c = true;
                        } else { cached_has_c = false; }
                    } else { cached_has_c = false; }
                } else { cached_t_ptr = std::ptr::null(); }
            }

            if cached_t_ptr.is_null() { continue; }

            let row = rec.row as usize;
            let t = unsafe { &*cached_t_ptr.add(row) };
            let gt = unsafe { &mut *cached_gt_ptr.add(row) };

            let (s, c) = parent_gt.rotation.sin_cos();
            let scaled_x = t.x * parent_gt.scale[0];
            let scaled_y = t.y * parent_gt.scale[1];

            gt.x = parent_gt.x + scaled_x * c - scaled_y * s;
            gt.y = parent_gt.y + scaled_x * s + scaled_y * c;
            gt.z = parent_gt.z + t.z;
            gt.rotation = parent_gt.rotation + t.rotation;
            gt.scale[0] = parent_gt.scale[0] * t.scale[0];
            gt.scale[1] = parent_gt.scale[1] * t.scale[1];

            if cached_has_c && depth < 32 {
                let children_comp = unsafe { &*cached_c_ptr.add(row) };
                for &child in &children_comp.0 {
                    if stack_ptr < 64 {
                        stack_entities[stack_ptr] = child.id as usize;
                        stack_gts[stack_ptr] = *gt;
                        stack_depths[stack_ptr] = depth + 1;
                        stack_ptr += 1;
                    }
                }
            }
        }
    });
}
