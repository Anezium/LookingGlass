#[cfg(test)]
mod r4_comparison_probe {
    use super::*;
    use sha2::{Digest,Sha256};
    #[test]
    fn randomized_map_sweep_fingerprint() {
        let data=std::env::var("LOOKING_GLASS_TEST_DATA").unwrap();
        let mut assets=crate::assets::Assets::open(std::path::Path::new(&data)).unwrap();
        for name in ["pandemonium","hedge3"] {
            let map=crate::bsp::Bsp::parse(&assets.read(&format!("maps/{name}.bsp")).unwrap()).unwrap();
            let mut hash=Sha256::new(); let mut contacts=0; let mut seed=0x17498aef_u32;
            let mut rand=||{seed=seed.wrapping_mul(1664525).wrapping_add(1013904223);(seed>>8) as f32/16777216.};
            for (mask,mut world) in [(0,World::from_bsp(&map).unwrap()),(1,World::actor_world(&map).unwrap())] {
                world.set_dynamic(vec![Collider::box_bounds(Vec3::splat(-40.),Vec3::splat(40.))]);
                world.set_settled_supports(vec![Collider::box_bounds(Vec3::splat(80.),Vec3::splat(120.))]);
                world.set_weapon_obstacles(vec![crate::combat::Target{id:7,center:Vec3::splat(200.),half:Vec3::splat(20.)}]);
                for i in 0..12000 {
                    let a=world.min+(world.max-world.min)*vec3(rand(),rand(),rand());
                    let b=if i%2==0 {a+vec3(rand()-0.5,rand()-0.5,rand()-0.5)*256.}
                        else {world.min+(world.max-world.min)*vec3(rand(),rand(),rand())};
                    let half=match i%4 {0=>Vec3::ZERO,1=>Vec3::splat(0.5),2=>PLAYER_HALF,_=>vec3(rand()*32.,rand()*32.,rand()*48.)};
                    let t=world.sweep(a,b,half); contacts+=usize::from(t.fraction<1. || t.start_solid);
                    for word in [mask,t.fraction.to_bits(),t.normal.x.to_bits(),t.normal.y.to_bits(),t.normal.z.to_bits(),t.start_solid as u32,t.all_solid as u32] {hash.update(word.to_le_bytes());}
                }
            }
            println!("R4_SWEEP {}",serde_json::json!({"map":name,"sweeps":24000,"contacts":contacts,"sha256":format!("{:x}",hash.finalize())}));
        }
    }
}
