"""Compile the actual Vita input function with a host SceCtrl stub; no device/SDK calls."""
import subprocess,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def function(text,name):
 start=text.index('fn '+name+'(');brace=text.index('{',start);depth=1;end=brace+1
 while depth:
  depth+=(text[end]=='{')-(text[end]=='}');end+=1
 return text[start:end]
class NativePad(unittest.TestCase):
 def test_cached_input_is_scoped_to_active_session(self):
  source=(ROOT/'src/replay/input.rs').read_text()
  native=source[source.index('#[cfg(target_os = "vita")]\npub fn vita_pad()'):]
  actual=function(native,'vita_pad').replace('#[link(name = "SceCtrl_stub")]','')
  harness='''use std::sync::atomic::{AtomicU8,AtomicU64,AtomicUsize,Ordering::Relaxed};
const OFF:u8=0;const RECORD:u8=1;const REPLAY:u8=2;
static MODE:AtomicU8=AtomicU8::new(OFF);static PAD:AtomicU64=AtomicU64::new(0);
static READS:AtomicUsize=AtomicUsize::new(0);
#[no_mangle] pub unsafe extern "C" fn sceCtrlPeekBufferPositive(_:i32,p:*mut u8,_:i32)->i32 {
 READS.fetch_add(1,Relaxed);std::ptr::write_bytes(p,0,32);
 std::ptr::copy_nonoverlapping([7u8,8,9,10].as_ptr(),p.add(12),4);1
}
'''+function(source,'mode')+'\n'+function(source,'pad_bits')+'\n'+function(source,'bits_pad')+'\n'+actual+'''
#[test] fn modes_and_stop() {
 let injected=Some([128,0,4,255]);
 MODE.store(RECORD,Relaxed);PAD.store(pad_bits(injected),Relaxed);
 assert_eq!(vita_pad(),injected);assert_eq!(READS.load(Relaxed),0);
 MODE.store(REPLAY,Relaxed);PAD.store(0,Relaxed);
 assert_eq!(vita_pad(),None);assert_eq!(READS.load(Relaxed),0);
 MODE.store(RECORD,Relaxed);PAD.store(0,Relaxed);
 assert_eq!(vita_pad(),Some([7,8,9,10]));assert_eq!(READS.load(Relaxed),1);
 assert_eq!(bits_pad(PAD.load(Relaxed)),Some([7,8,9,10]));
 // A stopped session may leave cached bytes, but physical play must read hardware.
 MODE.store(OFF,Relaxed);PAD.store(pad_bits(injected),Relaxed);
 assert_eq!(vita_pad(),Some([7,8,9,10]));assert_eq!(READS.load(Relaxed),2);
}
'''
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);(root/'pad.rs').write_text(harness)
   build=subprocess.run(['/Users/anezium/.cargo/bin/rustc','+nightly-2026-10-11','--edition=2021','--test',str(root/'pad.rs'),'-o',str(root/'pad-test')],capture_output=True,text=True)
   self.assertEqual(build.returncode,0,build.stdout+build.stderr)
   result=subprocess.run([str(root/'pad-test')],capture_output=True,text=True)
   self.assertEqual(result.returncode,0,result.stdout+result.stderr)
if __name__=='__main__':unittest.main()
