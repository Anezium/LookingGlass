#[cfg(test)]
mod r4_allocation_probe {
    use super::*;
    fn arc_bytes<T>() -> usize {
        std::alloc::Layout::new::<[usize; 2]>()
            .extend(std::alloc::Layout::new::<T>()).unwrap().0.pad_to_align().size()
    }
    #[test]
    fn construction_and_offmain_begin_census() {
        let data=std::env::var("LOOKING_GLASS_TEST_DATA").unwrap();
        let mut assets=crate::assets::Assets::open(std::path::Path::new(&data)).unwrap();
        for name in ["pandemonium","hedge3"] {
            let map=crate::bsp::Bsp::parse(&assets.read(&format!("maps/{name}.bsp")).unwrap()).unwrap();
            crate::kernel_alloc::census_begin();
            let world=World::from_bsp(&map).unwrap();
            let construction=crate::kernel_alloc::census_finish();
            crate::kernel_alloc::census_begin();
            let (mut c,server)=channel();
            let plumbing=crate::kernel_alloc::census_finish();
            crate::kernel_alloc::census_begin();c.begin(&world);
            let begin=crate::kernel_alloc::census_finish();
            println!("R4_ALLOCATION {}",serde_json::json!({"map":name,"units":"requested host allocator bytes; gross positive requests, signed retained delta; allocator metadata excluded","construction":construction,"channel":plumbing,"begin":begin,"fixed_packet":std::mem::size_of::<Batch>(),"fixed_queues":2*queue::allocation_bytes::<Box<Batch>>(1),"fixed_alive":arc_bytes::<AtomicBool>(),"fixed_stats":arc_bytes::<Stats>()}));
            drop((c,server,world));
        }
    }
}
