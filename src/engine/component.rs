use glam::Mat4;
use crate::ecs::Entity;
use std::marker::PhantomData;

#[derive(Debug)]
#[repr(C)]
pub struct Link<T: 'static, const N: usize> {
    pub count: usize,
    pub targets: [Entity; N],
    _marker: PhantomData<T>,
}

impl<T: 'static, const N: usize> Default for Link<T, N> {
    fn default() -> Self {
        Self {
            count: 0,
            targets: [Entity { id: 0, generation: 0 }; N],
            _marker: PhantomData,
        }
    }
}

impl<T: 'static, const N: usize> Clone for Link<T, N> {
    fn clone(&self) -> Self {
        Self {
            count: self.count,
            targets: self.targets,
            _marker: PhantomData,
        }
    }
}

impl<T: 'static, const N: usize> Copy for Link<T, N> {}

impl<T: 'static, const N: usize> Link<T, N> {
    pub fn push(&mut self, target: Entity) -> bool {
        if self.count < N {
            self.targets[self.count] = target;
            self.count += 1;
            true
        } else {
            false
        }
    }
}

impl<T: 'static, const N: usize> serde::Serialize for Link<T, N> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeTuple;
        let mut seq = serializer.serialize_tuple(self.count + 1)?;
        seq.serialize_element(&self.count)?;
        for i in 0..N {
            seq.serialize_element(&self.targets[i])?;
        }
        seq.end()
    }
}

impl<'de, T: 'static, const N: usize> serde::Deserialize<'de> for Link<T, N> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T, const N: usize>(PhantomData<T>);
        impl<'de, T: 'static, const N: usize> serde::de::Visitor<'de> for Visitor<T, N> {
            type Value = Link<T, N>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a tuple containing count and targets")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let count = seq.next_element()?.unwrap_or(0);
                let mut targets = [Entity { id: 0, generation: 0 }; N];
                for i in 0..N {
                    targets[i] = seq.next_element()?.unwrap_or(Entity { id: 0, generation: 0 });
                }
                Ok(Link { count, targets, _marker: PhantomData })
            }
        }
        deserializer.deserialize_tuple(N + 1, Visitor(PhantomData))
    }
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct GPUDrivenSimulation {
    pub max_instances: f32,
    pub mesh_id: f32,
    pub shader_type: f32,
    pub speed: f32,
    pub size: f32,
    pub gravity: f32,
    pub noise_scale: f32,
    pub pad: f32,
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Obstacle;

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Billboard {
    pub active: u32,
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Transform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Transform2D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation: f32,
    pub scale: [f32; 2],
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct GlobalTransform2D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation: f32,
    pub scale: [f32; 2],
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct GlobalTransform {
    pub matrix: Mat4,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C, align(4))]
pub struct GPUInstanceTransform {
    pub translation: [f32; 3],
    pub rotation: [i16; 4],
    pub scale: f32,
}

impl Default for GPUInstanceTransform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0, 0, 0, 32767],
            scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct MeshHandle {
    pub id: f32,
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct ShaderHandle {
    pub id: f32,
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct DynamicMesh {
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,

    pub version: u32,

    pub color_version: u32,
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct MeshBVH {
    pub nodes: Vec<crate::engine::spatial_3d::BVHNode>,
    pub tri_indices: Vec<u32>,
    pub version: u32,
}

#[derive(Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct AngularVelocity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct SpriteMaterial {
    pub color: [f32; 4],
    pub texture_id: f32,
    pub uv_x: f32,
    pub uv_y: f32,
    pub uv_w: f32,
    pub uv_h: f32,
}

impl Default for SpriteMaterial {
    fn default() -> Self {
        Self {
            color: [1.0; 4],
            texture_id: 0.0,
            uv_x: 0.0,
            uv_y: 0.0,
            uv_w: 1.0,
            uv_h: 1.0,
        }
    }
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct SpriteAnimation {
    pub fps: f32,
    pub frame_count: f32,
    pub current_frame: f32,
    pub timer: f32,
    pub width_per_frame: f32,
}

impl Default for SpriteAnimation {
    fn default() -> Self {
        Self {
            fps: 10.0,
            frame_count: 1.0,
            current_frame: 0.0,
            timer: 0.0,
            width_per_frame: 1.0,
        }
    }
}

#[derive(Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Camera {
    pub zoom: f32,
    pub active: u32,
}

#[derive(Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Collider {
    pub radius: f32,
}

#[derive(Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Collider3D {
    pub radius: f32,
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct AABB {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub half_size: [f32; 2],
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Visibility {
    pub visible: u32,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Camera3D {
    pub fov: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    pub view_proj: [f32; 16],
    pub inv_view_proj: [f32; 16],
    pub camera_pos: [f32; 3],
    pub exposure: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            fov: 1.047,
            aspect: 1.777,
            near: 0.1,
            far: 1000.0,
            view_proj: Mat4::IDENTITY.to_cols_array(),
            inv_view_proj: Mat4::IDENTITY.to_cols_array(),
            camera_pos: [0.0, 0.0, 0.0],
            exposure: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct StandardMaterial {
    pub base_color: [f32; 4],
    pub emissive: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
    pub pad: [f32; 3],
}

impl Default for StandardMaterial {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            emissive: [0.0; 3],
            metallic: 0.0,
            roughness: 0.5,
            pad: [0.0; 3],
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct DirectionalLight {
    pub color: [f32; 3],
    pub intensity: f32,
    pub direction: [f32; 3],
    pub pad: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            intensity: 3.14,
            direction: [0.0, -1.0, 0.0],
            pad: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct AmbientLight {
    pub color: [f32; 3],
    pub intensity: f32,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct HemisphereLight {
    pub sky_color: [f32; 3],
    pub sky_intensity: f32,
    pub ground_color: [f32; 3],
    pub ground_intensity: f32,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct PointLight {
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub pad: [f32; 3],
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            intensity: 100.0,
            range: 10.0,
            pad: [0.0; 3],
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct FlyCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
    pub sensitivity: f32,
    pub active: u32,
}

impl Default for FlyCamera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            speed: 20.0,
            sensitivity: 0.005,
            active: 1,
        }
    }
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct BoxCollider2D {
    pub half_x: f32,
    pub half_y: f32,
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct CapsuleCollider2D {
    pub half_length: f32,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Shape2D {
    pub shape_type: f32,
    pub color_r: f32, pub color_g: f32, pub color_b: f32, pub color_a: f32,
    pub extents_x: f32, pub extents_y: f32,
    pub border_radius: f32,
    pub border_color_r: f32, pub border_color_g: f32, pub border_color_b: f32, pub border_color_a: f32,
    pub border_thickness: f32,

    pub grad_type: f32,
    pub grad_color_r: f32, pub grad_color_g: f32, pub grad_color_b: f32, pub grad_color_a: f32,
    pub grad_p0_x: f32, pub grad_p0_y: f32,
    pub grad_p1_x: f32, pub grad_p1_y: f32,
}
impl Default for Shape2D {
    fn default() -> Self {
        Self {
            shape_type: 1.0,
            color_r: 1.0, color_g: 1.0, color_b: 1.0, color_a: 1.0,
            extents_x: 0.5, extents_y: 0.5,
            border_radius: 0.0,
            border_color_r: 0.0, border_color_g: 0.0, border_color_b: 0.0, border_color_a: 1.0,
            border_thickness: 0.0,
            grad_type: 0.0,
            grad_color_r: 0.0, grad_color_g: 0.0, grad_color_b: 0.0, grad_color_a: 0.0,
            grad_p0_x: 0.0, grad_p0_y: 0.0,
            grad_p1_x: 0.0, grad_p1_y: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct NetworkNode {
    pub connection_ids: [f32; 4],
    pub connection_gens: [f32; 4],
    pub flow_directions: [f32; 4],
}
impl Default for NetworkNode {
    fn default() -> Self {
        Self {
            connection_ids: [u32::MAX as f32; 4],
            connection_gens: [0.0; 4],
            flow_directions: [0.0; 4],
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct PathFollower {
    pub target_id: f32,
    pub target_gen: f32,
    pub speed: f32,
    pub progress: f32,
    pub active: f32,
}
impl Default for PathFollower {
    fn default() -> Self {
        Self {
            target_id: u32::MAX as f32,
            target_gen: 0.0,
            speed: 1.0,
            progress: 0.0,
            active: 1.0,
        }
    }
}

pub mod array_serde_120 {
    use serde::{Serialize, Deserialize, Serializer, Deserializer};
    pub fn serialize<S>(array: &[f32; 120], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        array[..].serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[f32; 120], D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec = Vec::<f32>::deserialize(deserializer)?;
        let mut array = [0.0; 120];
        let len = vec.len().min(120);
        array[..len].copy_from_slice(&vec[..len]);
        Ok(array)
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Text2D {
    pub font_size: f32,
    pub color: [f32; 4],
    pub alignment: f32,
    pub line_spacing: f32,
    pub len: f32,
    pub bold: f32,
    pub italic: f32,
    pub underline: f32,
    pub pad: f32,
    #[serde(with = "array_serde_120")]
    pub chars: [f32; 120],
}

impl Default for Text2D {
    fn default() -> Self {
        let mut chars = [0.0f32; 120];
        let default_text = "Artisan";
        for (i, b) in default_text.as_bytes().iter().enumerate() {
            chars[i] = *b as f32;
        }
        Self {
            font_size: 24.0,
            color: [1.0, 1.0, 1.0, 1.0],
            alignment: 0.0,
            line_spacing: 1.2,
            len: default_text.len() as f32,
            bold: 0.0,
            italic: 0.0,
            underline: 0.0,
            pad: 0.0,
            chars,
        }
    }
}
