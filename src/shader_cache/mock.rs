//! Host model of the public Vita GL lifetime and postponed compilation contract.
use super::*;
#[derive(Default)]
struct S {kind:u32,binary:bool,source:Vec<u8>,dirty:bool,refs:usize}
#[derive(Default)]
struct P {attached:Vec<u32>,linked:bool}
#[derive(Default)]
struct Mock {next:u32,shaders:BTreeMap<u32,S>,programs:BTreeMap<u32,P>,compiled:usize,binary_calls:usize,fail:bool}
thread_local! {static MOCK:RefCell<Mock>=RefCell::default();}
#[allow(non_upper_case_globals)]
pub static glsl_sema_mode:u32=2;
pub fn reset(){MOCK.with(|m|*m.borrow_mut()=Mock::default());}
pub fn compiled()->usize {MOCK.with(|m|m.borrow().compiled)}
pub fn binary_calls()->usize {MOCK.with(|m|m.borrow().binary_calls)}
pub fn fail_next_load(){MOCK.with(|m|m.borrow_mut().fail=true);}
pub fn live()->(usize,usize){MOCK.with(|m|(m.borrow().shaders.len(),m.borrow().programs.len()))}
pub unsafe fn glCreateShader(kind:u32)->u32 {MOCK.with(|m|{let mut m=m.borrow_mut();m.next+=1;let id=m.next;m.shaders.insert(id,S{kind,..Default::default()});id})}
pub unsafe fn glShaderSource(shader:u32,count:i32,strings:*const *const c_char,_lengths:*const i32){assert_eq!(count,1);MOCK.with(|m|m.borrow_mut().shaders.get_mut(&shader).unwrap().source=CStr::from_ptr(*strings).to_bytes().to_vec());}
pub unsafe fn glCompileShader(_shader:u32){}
pub unsafe fn glDeleteShader(shader:u32){MOCK.with(|m|{let mut m=m.borrow_mut();if m.shaders[&shader].refs==0{m.shaders.remove(&shader);}else{m.shaders.get_mut(&shader).unwrap().dirty=true;}});}
pub unsafe fn glCreateProgram()->u32{MOCK.with(|m|{let mut m=m.borrow_mut();m.next+=1;let id=m.next;m.programs.insert(id,P::default());id})}
pub unsafe fn glAttachShader(program:u32,shader:u32){MOCK.with(|m|{let mut m=m.borrow_mut();m.programs.get_mut(&program).unwrap().attached.push(shader);m.shaders.get_mut(&shader).unwrap().refs+=1;});}
pub unsafe fn glLinkProgram(program:u32){MOCK.with(|m|{let mut m=m.borrow_mut();let ids=m.programs[&program].attached.clone();assert_eq!(ids.len(),2);assert!(matches!(m.shaders[&ids[0]].kind,VERTEX|0x890e));assert!(matches!(m.shaders[&ids[1]].kind,FRAGMENT|0x890f));let binary=m.shaders[&ids[0]].binary;if binary&&m.fail{m.fail=false;return;}if !binary{assert!(!m.shaders[&ids[0]].source.is_empty());assert!(!m.shaders[&ids[1]].source.is_empty());m.compiled+=1;}m.programs.get_mut(&program).unwrap().linked=true;});}
pub unsafe fn glDeleteProgram(program:u32){MOCK.with(|m|{let mut m=m.borrow_mut();let p=m.programs.remove(&program).unwrap();for id in p.attached{let s=m.shaders.get_mut(&id).unwrap();s.refs-=1;if s.refs==0&&s.dirty{m.shaders.remove(&id);}}});}
pub unsafe fn glGetProgramiv(program:u32,pname:u32,value:*mut i32){*value=MOCK.with(|m|match pname {LINK_STATUS=>m.borrow().programs[&program].linked as i32,BINARY_LENGTH=>super::super::tests::fake_program().len() as i32,_=>panic!()});}
pub unsafe fn glGetShaderiv(shader:u32,_pname:u32,value:*mut i32){*value=MOCK.with(|m|m.borrow().shaders[&shader].binary as i32);}
pub unsafe fn glGetProgramBinary(_program:u32,size:i32,length:*mut i32,_format:*mut u32,data:*mut c_void){let b=super::super::tests::fake_program();assert_eq!(size as usize,b.len());std::ptr::copy_nonoverlapping(b.as_ptr(),data.cast(),b.len());*length=b.len() as i32;}
pub unsafe fn glShaderBinary(_count:i32,shaders:*const u32,_format:u32,_data:*const c_void,_len:i32){MOCK.with(|m|{let mut m=m.borrow_mut();m.binary_calls+=1;m.shaders.get_mut(&*shaders).unwrap().binary=true;});}
pub unsafe fn sceGxmProgramCheck(_program:*const c_void)->i32{0}
pub unsafe fn sceGxmProgramGetParameterCount(_program:*const c_void)->u32{100}
pub unsafe fn sceGxmProgramGetType(program:*const c_void)->u32{*(program.cast::<u8>().add(16)) as u32}

pub unsafe fn get_proc(_name:*const c_char)->*mut c_void {1usize as *mut c_void}
