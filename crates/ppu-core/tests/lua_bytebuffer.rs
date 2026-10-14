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

/// Wide writes against the w8 byte sequence they replace: same bytes, and the
/// returned previous value pins down exactly which bytes changed, so a caller
/// doing per-byte region tracking (VRAM words, register/palette versions)
/// reaches the same result with one call.
#[test]
fn wide_writes_match_their_byte_sequence() {
    let mut e = LuaEngine::new();
    e.set_source(
        r#"
  local b=bytebuffer(0)
  assert(b.w32(0,0x11223344)==0)
  assert(b.r8(0)==0x44 and b.r8(1)==0x33 and b.r8(2)==0x22 and b.r8(3)==0x11)
  assert(b.w16(1,0xbeef)==0x2233 and b.r32(0)==0x11beef44)
  assert(b.w16(0x10,0x12345)==0 and b.r16(0x10)==0x2345 and b.r8(0x12)==0)
  assert(b.w16(0x20,-2)==0 and b.r16(0x20)==0xfffe)
  assert(b.w64(0x30,-1)==0 and b.r64(0x30)==-1 and b.w64(0x30,-1)==-1)
  assert(b.w32(0x40,0xffffffff)==0 and b.w32(0x40,0)==0xffffffff)
  assert(b.w32(0x48,0x123456789)==0 and b.r32(0x48)==0x23456789 and b.r8(0x4c)==0)

  local g=bytebuffer(0)
  g.w64(100,1)
  assert(pcall(g.copy,0,100,8) and not pcall(g.copy,0,101,8))
  assert(not pcall(g.w16,33554431,0) and not pcall(g.w64,33554425,0))
  assert(not pcall(g.w64,math.maxinteger,0) and not pcall(g.w32,-1,0))
  assert(not pcall(g.w16,0,{}) and not pcall(g.w64,0))

  local P,V=0x20,0x60
  local C=V+65536
  local function tracker()
    local t={dirty={},reg=0,pal=0}
    function t.byte(a)
      if a>=V and a<V+65536 then t.dirty[(a-V)//2]=true
      elseif a>=P and a<P+64 then t.reg=t.reg+1
      elseif a>=C and a<C+512 then t.pal=t.pal+1 end
    end
    return t
  end
  local A,B=bytebuffer(0),bytebuffer(0)
  local ta,tb=tracker(),tracker()
  local wide={[2]=B.w16,[4]=B.w32,[8]=B.w64}
  local mask={[2]=0xffff,[4]=0xffffffff,[8]=-1}
  local function same()
    assert(ta.reg==tb.reg and ta.pal==tb.pal)
    for k in pairs(ta.dirty)do assert(tb.dirty[k])end
    for k in pairs(tb.dirty)do assert(ta.dirty[k])end
  end
  for _,c in ipairs({
    {0x5f,0x1234,2},{V+65535,0xabcd,2},{V+65533,0x0102030405060708,8},
    {V+1,0x11223344,4},{V+1,0x11223344,4},{V+1,0x11ff3344,4},
    {P-4,-1,8},{C+510,0x7fff,2},{C+511,0xffff,2},{V+10,-2,4},
    {V+20,0x123456789,4},{V+30,-1,8},{V+30,-1,8},{V+33,0x0100000000000001,8},
  })do
    local a,v,n=c[1],c[2],c[3]
    for i=0,n-1 do
      local x=(v>>(8*i))&255
      if A.w8(a+i,x)~=x then ta.byte(a+i)end
    end
    local diff=wide[n](a,v)~(v&mask[n])
    for i=0,n-1 do if (diff>>(8*i))&255~=0 then tb.byte(a+i)end end
    same()
  end
  assert(ta.reg>0 and ta.pal>0 and next(ta.dirty))
  for a=0,C+520 do assert(A.r8(a)==B.r8(a))end
  done=1234
  function frame(t,f)cgram[0]=done end
 "#,
    )
    .unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.memory().cgram[0], 1234);
}
