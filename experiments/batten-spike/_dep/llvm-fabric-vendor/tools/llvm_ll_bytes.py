#!/usr/bin/env python3
"""Real-LLVM byte-size comparison for experiment (c).

Generates the same toy shapes via llvmlite's IR builder and prints the
.ll text sizes, to compare against llvm-fabric's textual format and the
plain-SSA baseline. Byte sizes only — timings are NOT comparable across
implementations (llvmlite builds an in-memory module in C++; we print
from our own data structures).

Caveat kept honest: llvmlite quotes value names (%".3"), which inflates
LLVM's byte count a little (~1 byte/name over unquoted).
"""
from llvmlite import ir

I32 = ir.IntType(32)

def chain_ll(n):
    m = ir.Module(name="chain")
    fn = ir.Function(m, ir.FunctionType(I32, [I32]), name="f")
    b = ir.IRBuilder(fn.append_basic_block("entry"))
    prev = fn.args[0]
    for _ in range(n):
        prev = b.add(prev, I32(1))
    b.ret(prev)
    return str(m)

def diamonds_ll(n):
    m = ir.Module(name="diamonds")
    fn = ir.Function(m, ir.FunctionType(I32, [I32]), name="f")
    v = fn.args[0]
    b = ir.IRBuilder(fn.append_basic_block("entry"))
    for i in range(n):
        t = fn.append_basic_block(f"t{i}")
        e = fn.append_basic_block(f"e{i}")
        j = fn.append_basic_block(f"j{i}")
        b.cbranch(b.icmp_signed("<", v, I32(0)), t, e)
        b.position_at_end(t); vt = b.add(v, I32(1)); b.branch(j)
        b.position_at_end(e); ve = b.add(v, I32(2)); b.branch(j)
        b.position_at_end(j)
        v = b.phi(I32); v.add_incoming(vt, t); v.add_incoming(ve, e)
    b.ret(v)
    return str(m)

if __name__ == "__main__":
    for name, s in [
        ("chain-50", chain_ll(50)), ("chain-200", chain_ll(200)), ("chain-800", chain_ll(800)),
        ("diamonds-10", diamonds_ll(10)), ("diamonds-40", diamonds_ll(40)), ("diamonds-160", diamonds_ll(160)),
    ]:
        print(f"{name}: {len(s)} bytes")
