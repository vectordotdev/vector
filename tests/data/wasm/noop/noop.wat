(module
  (type (;0;) (func (result i32)))
  (type (;1;) (func))
  (type (;2;) (func (param i32)))
  (type (;3;) (func (param i32 i32 i32 i32)))
  (func $init (type 0) (result i32)
    (local i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 0
    global.set 0
    local.get 0
    i32.const 16
    i32.add
    call $_ZN11vector_wasm12Registration9transform17h642f96492738fb4dE
    local.get 0
    i32.const 8
    i32.add
    local.get 0
    i32.load offset=16
    local.get 0
    i32.load8_u offset=20
    i32.const 1
    i32.and
    i32.const 1
    call $_ZN11vector_wasm12Registration8set_wasi17h72d95cdd07d7683eE
    local.get 0
    i32.const 32
    i32.add
    global.set 0
    local.get 0
    i32.const 24
    i32.add)
  (func $process (type 0) (result i32)
    i32.const 0)
  (func $shutdown (type 1))
  (func $_ZN11vector_wasm12Registration9transform17h642f96492738fb4dE (type 2) (param i32)
    local.get 0
    i32.const 0
    i32.store8 offset=4
    local.get 0
    i32.const 0
    i32.store)
  (func $_ZN11vector_wasm12Registration8set_wasi17h72d95cdd07d7683eE (type 3) (param i32 i32 i32 i32)
    local.get 0
    local.get 3
    i32.store8 offset=4
    local.get 0
    local.get 1
    i32.store)
  (table (;0;) 1 1 funcref)
  (memory (;0;) 16)
  (global (;0;) (mut i32) (i32.const 1048576))
  (global (;1;) i32 (i32.const 1048576))
  (global (;2;) i32 (i32.const 1048576))
  (export "memory" (memory 0))
  (export "init" (func $init))
  (export "process" (func $process))
  (export "shutdown" (func $shutdown))
  (export "__data_end" (global 1))
  (export "__heap_base" (global 2)))
