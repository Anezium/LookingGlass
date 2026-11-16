//! Exact field uploads for a private, owned OpenGL program. Locations are
//! resolved once; interruptions invalidate values without reallocating storage.
use macroquad::miniquad::{gl::*, RenderingBackend, ShaderMeta, UniformType};
use std::{ffi::CString, ops::Range};

struct Field {
    name: CString,
    kind: UniformType,
    count: i32,
    range: Range<usize>,
    location: i32,
}
#[derive(Default, Clone, Copy)]
pub(crate) struct Traffic { pub calls: u64, pub bytes: u64 }
pub(crate) struct Cache {
    fields: Vec<Field>,
    previous: Vec<u32>,
    resolved: bool,
    valid: bool,
}
impl Cache {
    pub(crate) fn new(meta: ShaderMeta) -> Self {
        let mut length = 0;
        let fields = meta.uniforms.uniforms.into_iter().map(|u| {
            assert!(matches!(u.uniform_type, UniformType::Float1 | UniformType::Float2 |
                UniformType::Float3 | UniformType::Float4 | UniformType::Mat4));
            let start = length;
            length += u.uniform_type.size() / 4 * u.array_count;
            Field { name: CString::new(u.name).unwrap(), kind: u.uniform_type,
                count: u.array_count as i32, range: start..length, location: -1 }
        }).collect();
        Self { fields, previous: vec![0; length], resolved: false, valid: false }
    }
    pub(crate) fn interrupt(&mut self) { self.valid = false; }
    /// The caller has applied this cache's private pipeline/program. No program,
    /// sampler, buffer, texture, depth or blend state changes here.
    pub(crate) fn apply(&mut self, ctx: &mut dyn RenderingBackend, values: &[f32], enabled: bool) -> Traffic {
        crate::game_thread::assert_current();
        assert_eq!(values.len(), self.previous.len());
        if !self.resolved {
            let mut program = 0;
            unsafe { glGetIntegerv(0x8B8D, &mut program); }
            assert!(program != 0, "uniform upload requires the owned OpenGL program");
            for field in &mut self.fields {
                field.location = unsafe { glGetUniformLocation(program as u32, field.name.as_ptr()) };
            }
            self.resolved = true;
        }
        if !enabled { ctx.apply_uniforms_from_bytes(values.as_ptr().cast(), values.len() * 4); }
        let mut traffic = Traffic::default();
        for field in &self.fields {
            let changed = !enabled || !self.valid || field.range.clone()
                .any(|i| self.previous[i] != values[i].to_bits());
            if field.location != -1 && changed {
                if enabled { unsafe {
                    let data = values.as_ptr().add(field.range.start);
                    match field.kind {
                        UniformType::Float1 => glUniform1fv(field.location, field.count, data),
                        UniformType::Float2 => glUniform2fv(field.location, field.count, data),
                        UniformType::Float3 => glUniform3fv(field.location, field.count, data),
                        UniformType::Float4 => glUniform4fv(field.location, field.count, data),
                        UniformType::Mat4 => glUniformMatrix4fv(field.location, field.count, 0, data),
                        _ => unreachable!(),
                    }
                } }
                traffic.calls += 1;
                traffic.bytes += (field.range.len() * 4) as u64;
            }
        }
        if enabled {
            for (dst, src) in self.previous.iter_mut().zip(values) { *dst = src.to_bits(); }
        }
        self.valid = enabled;
        traffic
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_float_fields_and_arrays_cover_the_payload_without_padding() {
        use macroquad::miniquad::{UniformBlockLayout, UniformDesc};
        let mut c = Cache::new(ShaderMeta { images: vec![], uniforms: UniformBlockLayout {
            uniforms: vec![UniformDesc::new("M", UniformType::Mat4),
                UniformDesc::new("V", UniformType::Float3).array(2),
                UniformDesc::new("S", UniformType::Float1)] } });
        assert_eq!(c.previous.len(), 23);
        assert_eq!(c.fields.iter().map(|f| f.range.clone()).collect::<Vec<_>>(), vec![0..16,16..22,22..23]);
        c.valid = true; c.interrupt(); assert!(!c.valid);
        assert_ne!(0_f32.to_bits(), (-0_f32).to_bits());
        assert_ne!(f32::from_bits(0x7fc00001).to_bits(), f32::from_bits(0x7fc00002).to_bits());
    }
}
