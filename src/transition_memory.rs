//! Scene replacement high-water samples. Both scenes are alive at before_drop;
//! after_drop still conservatively includes the outgoing non-scene resources.
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Snapshot {
    heap_used: Option<usize>,
    heap_free: Option<usize>,
    heap_arena: Option<usize>,
    vgl_ram_free: Option<usize>,
    vgl_vram_free: Option<usize>,
    vgl_phycont_free: Option<usize>,
}
fn sample() -> Snapshot {
    #[cfg(target_os = "vita")]
    {
        // VitaSDK malloc.h: ten size_t fields; vitaGL.h enum values 0/1/2.
        #[repr(C)]
        struct MallInfo {
            arena: usize, ordblks: usize, smblks: usize, hblks: usize,
            hblkhd: usize, usmblks: usize, fsmblks: usize, uordblks: usize,
            fordblks: usize, keepcost: usize,
        }
        extern "C" {
            fn mallinfo() -> MallInfo;
            fn vglMemFree(kind: i32) -> usize;
        }
        unsafe {
            let heap = mallinfo();
            Snapshot {
                heap_used: Some(heap.uordblks), heap_free: Some(heap.fordblks),
                heap_arena: Some(heap.arena), vgl_ram_free: Some(vglMemFree(1)),
                vgl_vram_free: Some(vglMemFree(0)), vgl_phycont_free: Some(vglMemFree(2)),
            }
        }
    }
    #[cfg(not(target_os = "vita"))]
    Snapshot { heap_used: None, heap_free: None, heap_arena: None,
        vgl_ram_free: None, vgl_vram_free: None, vgl_phycont_free: None }
}

pub(crate) fn replace<T>(old: &mut T, next: T, kind: &str, from: &str, to: &str) {
    // Exclude the entire transition frame from CPU/wall/phase timing before
    // touching allocators or formatting a log. No frame samples include this IO.
    crate::frame_profile::exclude_transition_frame();
    let time_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_millis();
    let before_drop = sample();
    *old = next;
    let after_drop = sample();
    #[derive(Serialize)]
    struct Record<'a> {
        time_ms: u128, kind: &'a str, from: &'a str, to: &'a str,
        before_drop: Snapshot, after_drop: Snapshot,
    }
    eprintln!("[lg/transition-memory] {}", serde_json::to_string(&Record {
        time_ms, kind, from, to, before_drop, after_drop,
    }).expect("transition memory serialization"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn replacement_drops_old_once_before_returning() {
        use std::{cell::Cell, rc::Rc};
        struct Item(Rc<Cell<usize>>);
        impl Drop for Item { fn drop(&mut self) { self.0.set(self.0.get() + 1); } }
        let drops = Rc::new(Cell::new(0));
        let mut old = Item(drops.clone());
        super::replace(&mut old, Item(drops.clone()), "test", "old", "new");
        assert_eq!(drops.get(), 1);
        drop(old);
        assert_eq!(drops.get(), 2);
        crate::frame_profile::begin_viewer_frame();
    }
}
