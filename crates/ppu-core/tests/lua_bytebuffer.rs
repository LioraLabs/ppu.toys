use ppu_core::LuaEngine;
#[test]
fn bytebuffer_reads_little_endian_and_preserves_overlapping_copies() {
    let mut e = LuaEngine::new();
    e.set_source(
        r#"
  local b=bytebuffer(8)
  b.load(0,"\001\127\128\255")
  assert(b.r8(2)==128 and b.r16(2)==65408 and b.r32(0)==0xff807f01)
  assert(b.w8(8,511)==0 and b.r8(8)==255)
  b.copy(1,0,4)
  assert(b.r8(1)==1 and b.r8(2)==127 and b.r8(3)==128 and b.r8(4)==255)
  b.fill(3,256,3);assert(b.r16(3)==0)
  b.load(0,"\255\255\255\255\255\255\255\255")
  assert(b.r64(0)==-1)
  assert(b.r8(1000)==0)
  assert(not pcall(b.w8,-1,0))
  assert(not pcall(b.load,33554432,"x"))
  assert(not pcall(b.copy,0,-1,1))
  function frame(t,f)cgram[0]=b.r16(0)end
 "#,
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().cgram[0], 32767);
}
