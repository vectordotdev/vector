(module
  (type (;0;) (func (param i32)))
  (type (;1;) (func (param i32) (result i64)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32 i32 i32)))
  (type (;4;) (func (param i32 i32)))
  (type (;5;) (func (param i32 i32 i32)))
  (type (;6;) (func (param i32 i32 i32) (result i32)))
  (type (;7;) (func (param i64 i64)))
  (type (;8;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;9;) (func))
  (type (;10;) (func (param i64 i64) (result i32)))
  (type (;11;) (func (param i64) (result i32)))
  (type (;12;) (func (param i32 i32 i32 i32 i32)))
  (type (;13;) (func (result i32)))
  (type (;14;) (func (param i32) (result i32)))
  (type (;15;) (func (param i32 i32 i32 i32 i32) (result i32)))
  (type (;16;) (func (param i64 i32 i32) (result i32)))
  (type (;17;) (func (param i32 i32 i32 i32 i32 i32) (result i32)))
  (type (;18;) (func (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
  (import "env" "register" (func $register (type 7)))
  (import "wasi_snapshot_preview1" "fd_write" (func $_ZN4wasi13lib_generated22wasi_snapshot_preview18fd_write17h62539b3299a4581fE (type 8)))
  (func $init (type 9)
    (local i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 0
    global.set 0
    local.get 0
    call $_ZN11vector_wasm12registration12Registration9transform17h71a5e930d73f4337E
    i32.store offset=8
    local.get 0
    i32.const 8
    i32.add
    call $_ZN11vector_wasm12registration12Registration8register17h6a69957fe30ccf28E
    local.get 0
    i32.const 16
    i32.add
    global.set 0)
  (func $process (type 10) (param i64 i64) (result i32)
    i32.const 1048576
    i32.const 12
    i32.const 1048620
    call $_ZN3std9panicking11begin_panic17hc084a3050dc4f7eaE
    unreachable)
  (func $shutdown (type 9))
  (func $allocate_buffer (type 11) (param i64) (result i32)
    block  ;; label = @1
      local.get 0
      i32.wrap_i64
      i32.const -1
      i32.gt_s
      br_if 0 (;@1;)
      call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$11allocate_in28_$u7b$$u7b$closure$u7d$$u7d$17hde9d194e8c83759bE.llvm.7458102959860666061
      unreachable
    end
    i32.const 1)
  (func $drop_buffer (type 4) (param i32 i32))
  (func $_ZN3std9panicking11begin_panic17hc084a3050dc4f7eaE (type 5) (param i32 i32 i32)
    (local i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    local.get 1
    i32.store offset=12
    local.get 3
    local.get 0
    i32.store offset=8
    local.get 3
    i32.const 8
    i32.add
    i32.const 1048636
    i32.const 0
    local.get 2
    call $_ZN4core5panic8Location6caller17hba7ec45f0d210bdeE
    call $_ZN3std9panicking20rust_panic_with_hook17h8bf13b9f643a54b1E
    unreachable)
  (func $_ZN4core3ptr13drop_in_place17h5c9b794b2099b1eaE (type 0) (param i32))
  (func $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$3get17h003b888122c564eaE (type 4) (param i32 i32)
    block  ;; label = @1
      local.get 1
      i32.load
      br_if 0 (;@1;)
      call $_ZN3std7process5abort17h1646aa60de17f512E
      unreachable
    end
    local.get 0
    i32.const 1048656
    i32.store offset=4
    local.get 0
    local.get 1
    i32.store)
  (func $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$8take_box17hfc597a3aff2741abE (type 4) (param i32 i32)
    (local i32 i32)
    local.get 1
    i32.load
    local.set 2
    local.get 1
    i32.const 0
    i32.store
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.eqz
        br_if 0 (;@2;)
        local.get 1
        i32.load offset=4
        local.set 3
        i32.const 8
        i32.const 4
        call $__rust_alloc
        local.tee 1
        i32.eqz
        br_if 1 (;@1;)
        local.get 1
        local.get 3
        i32.store offset=4
        local.get 1
        local.get 2
        i32.store
        local.get 0
        i32.const 1048656
        i32.store offset=4
        local.get 0
        local.get 1
        i32.store
        return
      end
      call $_ZN3std7process5abort17h1646aa60de17f512E
      unreachable
    end
    i32.const 8
    i32.const 4
    call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
    unreachable)
  (func $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$11allocate_in28_$u7b$$u7b$closure$u7d$$u7d$17hde9d194e8c83759bE.llvm.7458102959860666061 (type 9)
    call $_ZN5alloc7raw_vec17capacity_overflow17h60fd539dfca5134dE
    unreachable)
  (func $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17h3d1c1bb0748b11daE (type 1) (param i32) (result i64)
    i64.const 1229646359891580772)
  (func $__rust_alloc (type 2) (param i32 i32) (result i32)
    (local i32)
    local.get 0
    local.get 1
    call $__rdl_alloc
    local.set 2
    local.get 2
    return)
  (func $__rust_dealloc (type 5) (param i32 i32 i32)
    local.get 0
    local.get 1
    local.get 2
    call $__rdl_dealloc
    return)
  (func $__rust_realloc (type 8) (param i32 i32 i32 i32) (result i32)
    (local i32)
    local.get 0
    local.get 1
    local.get 2
    local.get 3
    call $__rdl_realloc
    local.set 4
    local.get 4
    return)
  (func $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E (type 5) (param i32 i32 i32)
    (local i32)
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 0
          i32.const 4
          i32.add
          i32.load
          local.tee 3
          local.get 1
          i32.sub
          local.get 2
          i32.ge_u
          br_if 0 (;@3;)
          local.get 1
          local.get 2
          i32.add
          local.tee 2
          local.get 1
          i32.lt_u
          br_if 2 (;@1;)
          local.get 3
          i32.const 1
          i32.shl
          local.tee 1
          local.get 2
          local.get 1
          local.get 2
          i32.gt_u
          select
          local.tee 1
          i32.const 0
          i32.lt_s
          br_if 2 (;@1;)
          block  ;; label = @4
            block  ;; label = @5
              local.get 3
              br_if 0 (;@5;)
              local.get 1
              i32.const 1
              call $__rust_alloc
              local.set 2
              br 1 (;@4;)
            end
            local.get 0
            i32.load
            local.get 3
            i32.const 1
            local.get 1
            call $__rust_realloc
            local.set 2
          end
          local.get 2
          i32.eqz
          br_if 1 (;@2;)
          local.get 0
          local.get 2
          i32.store
          local.get 0
          i32.const 4
          i32.add
          local.get 1
          i32.store
        end
        return
      end
      local.get 1
      i32.const 1
      call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
      unreachable
    end
    call $_ZN5alloc7raw_vec17capacity_overflow17h60fd539dfca5134dE
    unreachable)
  (func $_ZN77_$LT$alloc..raw_vec..RawVec$LT$T$C$A$GT$$u20$as$u20$core..ops..drop..Drop$GT$4drop17hf042fdaaf184435fE (type 0) (param i32)
    (local i32)
    block  ;; label = @1
      local.get 0
      i32.const 4
      i32.add
      i32.load
      local.tee 1
      i32.eqz
      br_if 0 (;@1;)
      local.get 0
      i32.load
      local.get 1
      i32.const 1
      call $__rust_dealloc
    end)
  (func $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E (type 3) (param i32 i32 i32 i32)
    (local i32)
    global.get 0
    i32.const 96
    i32.sub
    local.tee 4
    global.set 0
    local.get 4
    local.get 1
    i32.store offset=8
    local.get 4
    local.get 3
    i32.store offset=12
    block  ;; label = @1
      local.get 1
      local.get 3
      i32.ne
      br_if 0 (;@1;)
      local.get 0
      local.get 2
      local.get 1
      call $memcpy
      drop
      local.get 4
      i32.const 96
      i32.add
      global.set 0
      return
    end
    local.get 4
    i32.const 40
    i32.add
    i32.const 20
    i32.add
    i32.const 5
    i32.store
    local.get 4
    i32.const 52
    i32.add
    i32.const 6
    i32.store
    local.get 4
    i32.const 16
    i32.add
    i32.const 20
    i32.add
    i32.const 3
    i32.store
    local.get 4
    local.get 4
    i32.const 8
    i32.add
    i32.store offset=64
    local.get 4
    local.get 4
    i32.const 12
    i32.add
    i32.store offset=68
    local.get 4
    i32.const 72
    i32.add
    i32.const 20
    i32.add
    i32.const 0
    i32.store
    local.get 4
    i64.const 3
    i64.store offset=20 align=4
    local.get 4
    i32.const 1048816
    i32.store offset=16
    local.get 4
    i32.const 6
    i32.store offset=44
    local.get 4
    i32.const 1048900
    i32.store offset=88
    local.get 4
    i64.const 1
    i64.store offset=76 align=4
    local.get 4
    i32.const 1048892
    i32.store offset=72
    local.get 4
    local.get 4
    i32.const 40
    i32.add
    i32.store offset=32
    local.get 4
    local.get 4
    i32.const 72
    i32.add
    i32.store offset=56
    local.get 4
    local.get 4
    i32.const 68
    i32.add
    i32.store offset=48
    local.get 4
    local.get 4
    i32.const 64
    i32.add
    i32.store offset=40
    local.get 4
    i32.const 16
    i32.add
    i32.const 1048976
    call $_ZN4core5panic8Location6caller17hba7ec45f0d210bdeE
    call $_ZN4core9panicking9panic_fmt17h98142caac1112f39E
    unreachable)
  (func $_ZN10serde_json3ser18format_escaped_str17h909b592d21f22459E (type 12) (param i32 i32 i32 i32 i32)
    (local i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 5
    global.set 0
    local.get 1
    i32.load
    local.tee 6
    local.get 6
    i32.const 8
    i32.add
    local.tee 7
    i32.load
    i32.const 1
    call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E
    local.get 7
    local.get 7
    i32.load
    local.tee 8
    i32.const 1
    i32.add
    i32.store
    local.get 8
    local.get 6
    i32.load
    i32.add
    i32.const 1
    i32.const 1049080
    i32.const 1
    call $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E
    local.get 3
    local.get 4
    i32.add
    local.set 9
    local.get 4
    i32.const -1
    i32.xor
    local.set 10
    local.get 3
    i32.const -1
    i32.add
    local.set 11
    i32.const 0
    local.set 12
    local.get 3
    local.set 8
    loop  ;; label = @1
      local.get 12
      local.set 13
      local.get 9
      local.get 8
      i32.sub
      local.set 12
      i32.const 0
      local.set 6
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      loop  ;; label = @10
                        block  ;; label = @11
                          local.get 12
                          local.get 6
                          i32.ne
                          br_if 0 (;@11;)
                          local.get 13
                          local.get 4
                          i32.eq
                          br_if 3 (;@8;)
                          local.get 5
                          local.get 4
                          i32.store offset=4
                          local.get 5
                          local.get 3
                          i32.store
                          local.get 5
                          local.get 13
                          i32.store offset=8
                          local.get 5
                          local.get 4
                          i32.store offset=12
                          local.get 13
                          i32.eqz
                          br_if 2 (;@9;)
                          block  ;; label = @12
                            local.get 13
                            local.get 4
                            i32.ge_u
                            br_if 0 (;@12;)
                            local.get 3
                            local.get 13
                            i32.add
                            i32.load8_s
                            i32.const -65
                            i32.gt_s
                            br_if 3 (;@9;)
                          end
                          local.get 5
                          local.get 5
                          i32.const 12
                          i32.add
                          i32.store offset=24
                          local.get 5
                          local.get 5
                          i32.const 8
                          i32.add
                          i32.store offset=20
                          local.get 5
                          local.get 5
                          i32.store offset=16
                          local.get 5
                          i32.const 16
                          i32.add
                          call $_ZN4core3str6traits101_$LT$impl$u20$core..slice..SliceIndex$LT$str$GT$$u20$for$u20$core..ops..range..Range$LT$usize$GT$$GT$5index28_$u7b$$u7b$closure$u7d$$u7d$17h48519807e89ab690E
                          unreachable
                        end
                        local.get 8
                        local.get 6
                        i32.add
                        local.set 7
                        local.get 6
                        i32.const 1
                        i32.add
                        local.set 6
                        local.get 7
                        i32.load8_u
                        local.tee 14
                        i32.const 1049308
                        i32.add
                        i32.load8_u
                        local.tee 7
                        i32.eqz
                        br_if 0 (;@10;)
                      end
                      local.get 13
                      local.get 6
                      i32.add
                      local.tee 12
                      i32.const -1
                      i32.add
                      local.tee 15
                      local.get 13
                      i32.le_u
                      br_if 3 (;@6;)
                      local.get 5
                      local.get 4
                      i32.store offset=4
                      local.get 5
                      local.get 3
                      i32.store
                      local.get 5
                      local.get 13
                      i32.store offset=8
                      local.get 5
                      local.get 15
                      i32.store offset=12
                      local.get 13
                      i32.eqz
                      br_if 2 (;@7;)
                      local.get 13
                      local.get 4
                      i32.eq
                      br_if 2 (;@7;)
                      local.get 13
                      local.get 4
                      i32.ge_u
                      br_if 4 (;@5;)
                      local.get 3
                      local.get 13
                      i32.add
                      i32.load8_s
                      i32.const -65
                      i32.gt_s
                      br_if 2 (;@7;)
                      br 4 (;@5;)
                    end
                    local.get 1
                    i32.load
                    local.tee 7
                    local.get 7
                    i32.const 8
                    i32.add
                    local.tee 8
                    i32.load
                    local.get 4
                    local.get 13
                    i32.sub
                    local.tee 6
                    call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E
                    local.get 8
                    local.get 8
                    i32.load
                    local.tee 12
                    local.get 6
                    i32.add
                    i32.store
                    local.get 12
                    local.get 7
                    i32.load
                    i32.add
                    local.get 6
                    local.get 3
                    local.get 13
                    i32.add
                    local.get 6
                    call $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E
                  end
                  local.get 1
                  i32.load
                  local.tee 6
                  local.get 6
                  i32.const 8
                  i32.add
                  local.tee 7
                  i32.load
                  i32.const 1
                  call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E
                  local.get 7
                  local.get 7
                  i32.load
                  local.tee 8
                  i32.const 1
                  i32.add
                  i32.store
                  local.get 8
                  local.get 6
                  i32.load
                  i32.add
                  i32.const 1
                  i32.const 1049080
                  i32.const 1
                  call $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E
                  local.get 0
                  i32.const 3
                  i32.store8
                  local.get 5
                  i32.const 32
                  i32.add
                  global.set 0
                  return
                end
                block  ;; label = @7
                  local.get 10
                  local.get 13
                  i32.add
                  local.get 6
                  i32.add
                  i32.eqz
                  br_if 0 (;@7;)
                  local.get 15
                  local.get 4
                  i32.ge_u
                  br_if 2 (;@5;)
                  local.get 11
                  local.get 13
                  i32.add
                  local.get 6
                  i32.add
                  i32.load8_s
                  i32.const -65
                  i32.le_s
                  br_if 2 (;@5;)
                end
                local.get 1
                i32.load
                local.tee 15
                local.get 15
                i32.const 8
                i32.add
                local.tee 16
                i32.load
                local.get 6
                i32.const -1
                i32.add
                local.tee 17
                call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E
                local.get 16
                local.get 16
                i32.load
                local.tee 18
                local.get 6
                i32.add
                i32.const -1
                i32.add
                i32.store
                local.get 18
                local.get 15
                i32.load
                i32.add
                local.get 17
                local.get 3
                local.get 13
                i32.add
                local.get 17
                call $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E
              end
              local.get 8
              local.get 6
              i32.add
              local.set 8
              local.get 7
              i32.const -92
              i32.add
              local.tee 6
              i32.const 25
              i32.le_u
              br_if 1 (;@4;)
              i32.const 1049094
              local.set 14
              local.get 7
              i32.const 34
              i32.eq
              br_if 2 (;@3;)
              br 3 (;@2;)
            end
            local.get 5
            local.get 5
            i32.const 12
            i32.add
            i32.store offset=24
            local.get 5
            local.get 5
            i32.const 8
            i32.add
            i32.store offset=20
            local.get 5
            local.get 5
            i32.store offset=16
            local.get 5
            i32.const 16
            i32.add
            call $_ZN4core3str6traits101_$LT$impl$u20$core..slice..SliceIndex$LT$str$GT$$u20$for$u20$core..ops..range..Range$LT$usize$GT$$GT$5index28_$u7b$$u7b$closure$u7d$$u7d$17h48519807e89ab690E
            unreachable
          end
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      block  ;; label = @10
                        local.get 6
                        br_table 0 (;@10;) 8 (;@2;) 8 (;@2;) 8 (;@2;) 8 (;@2;) 8 (;@2;) 6 (;@4;) 8 (;@2;) 8 (;@2;) 8 (;@2;) 5 (;@5;) 8 (;@2;) 8 (;@2;) 8 (;@2;) 8 (;@2;) 8 (;@2;) 8 (;@2;) 8 (;@2;) 4 (;@6;) 8 (;@2;) 8 (;@2;) 8 (;@2;) 3 (;@7;) 8 (;@2;) 2 (;@8;) 1 (;@9;) 0 (;@10;)
                      end
                      i32.const 1049092
                      local.set 14
                      br 6 (;@3;)
                    end
                    local.get 1
                    i32.load
                    local.set 6
                    local.get 5
                    i32.const 808482140
                    i32.store offset=16 align=1
                    local.get 5
                    local.get 14
                    i32.const 15
                    i32.and
                    i32.const 1049292
                    i32.add
                    i32.load8_u
                    i32.store8 offset=21
                    local.get 5
                    local.get 14
                    i32.const 4
                    i32.shr_u
                    i32.const 1049292
                    i32.add
                    i32.load8_u
                    i32.store8 offset=20
                    local.get 6
                    local.get 6
                    i32.const 8
                    i32.add
                    local.tee 7
                    i32.load
                    i32.const 6
                    call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E
                    local.get 7
                    local.get 7
                    i32.load
                    local.tee 14
                    i32.const 6
                    i32.add
                    i32.store
                    local.get 14
                    local.get 6
                    i32.load
                    i32.add
                    i32.const 6
                    local.get 5
                    i32.const 16
                    i32.add
                    i32.const 6
                    call $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E
                    br 7 (;@1;)
                  end
                  i32.const 1049082
                  local.set 14
                  br 4 (;@3;)
                end
                i32.const 1049084
                local.set 14
                br 3 (;@3;)
              end
              i32.const 1049086
              local.set 14
              br 2 (;@3;)
            end
            i32.const 1049088
            local.set 14
            br 1 (;@3;)
          end
          i32.const 1049090
          local.set 14
        end
        local.get 1
        i32.load
        local.tee 6
        local.get 6
        i32.const 8
        i32.add
        local.tee 7
        i32.load
        i32.const 2
        call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E
        local.get 7
        local.get 7
        i32.load
        local.tee 13
        i32.const 2
        i32.add
        i32.store
        local.get 13
        local.get 6
        i32.load
        i32.add
        i32.const 2
        local.get 14
        i32.const 2
        call $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E
        br 1 (;@1;)
      end
    end
    i32.const 1048992
    i32.const 40
    i32.const 1049064
    call $_ZN3std9panicking11begin_panic17h8d948eabdf427119E
    unreachable)
  (func $_ZN4core3str6traits101_$LT$impl$u20$core..slice..SliceIndex$LT$str$GT$$u20$for$u20$core..ops..range..Range$LT$usize$GT$$GT$5index28_$u7b$$u7b$closure$u7d$$u7d$17h48519807e89ab690E (type 0) (param i32)
    (local i32)
    local.get 0
    i32.load
    local.tee 1
    i32.load
    local.get 1
    i32.load offset=4
    local.get 0
    i32.load offset=4
    i32.load
    local.get 0
    i32.load offset=8
    i32.load
    call $_ZN4core3str16slice_error_fail17ha06f3354b25aeac4E
    unreachable)
  (func $_ZN4core3ptr13drop_in_place17h53dd4d82272064b7E (type 0) (param i32)
    (local i32 i32 i32 i32)
    block  ;; label = @1
      local.get 0
      i32.load
      local.tee 1
      i32.eqz
      br_if 0 (;@1;)
      block  ;; label = @2
        local.get 1
        i32.load
        local.tee 2
        i32.const 1
        i32.gt_u
        br_if 0 (;@2;)
        block  ;; label = @3
          block  ;; label = @4
            local.get 2
            br_table 0 (;@4;) 1 (;@3;) 0 (;@4;)
          end
          local.get 1
          i32.const 8
          i32.add
          i32.load
          local.tee 2
          i32.eqz
          br_if 1 (;@2;)
          local.get 1
          i32.load offset=4
          local.get 2
          i32.const 1
          call $__rust_dealloc
          br 1 (;@2;)
        end
        local.get 1
        i32.load8_u offset=4
        i32.const 2
        i32.lt_u
        br_if 0 (;@2;)
        local.get 1
        i32.const 8
        i32.add
        i32.load
        local.tee 2
        i32.load
        local.get 2
        i32.load offset=4
        i32.load
        call_indirect (type 0)
        block  ;; label = @3
          local.get 2
          i32.load offset=4
          local.tee 3
          i32.load offset=4
          local.tee 4
          i32.eqz
          br_if 0 (;@3;)
          local.get 2
          i32.load
          local.get 4
          local.get 3
          i32.load offset=8
          call $__rust_dealloc
        end
        local.get 1
        i32.load offset=8
        i32.const 12
        i32.const 4
        call $__rust_dealloc
      end
      local.get 0
      i32.load
      i32.const 20
      i32.const 4
      call $__rust_dealloc
    end)
  (func $_ZN5serde3ser12SerializeMap15serialize_entry17h7292117eedface93E (type 8) (param i32 i32 i32 i32) (result i32)
    (local i32 i32 i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 4
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        local.get 0
        i32.load8_u
        i32.const 1
        i32.eq
        br_if 0 (;@2;)
        block  ;; label = @3
          local.get 0
          i32.load8_u offset=1
          i32.const 1
          i32.eq
          br_if 0 (;@3;)
          local.get 0
          i32.load offset=4
          i32.load
          local.tee 5
          local.get 5
          i32.const 8
          i32.add
          local.tee 6
          i32.load
          i32.const 1
          call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E
          local.get 6
          local.get 6
          i32.load
          local.tee 7
          i32.const 1
          i32.add
          i32.store
          local.get 7
          local.get 5
          i32.load
          i32.add
          i32.const 1
          i32.const 1049081
          i32.const 1
          call $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E
        end
        local.get 4
        i32.const 0
        i32.store offset=24
        local.get 4
        i32.const 24
        i32.add
        call $_ZN4core3ptr13drop_in_place17h53dd4d82272064b7E
        local.get 0
        i32.const 2
        i32.store8 offset=1
        local.get 4
        i32.const 16
        i32.add
        local.get 0
        i32.load offset=4
        local.get 4
        local.get 1
        local.get 2
        call $_ZN10serde_json3ser18format_escaped_str17h909b592d21f22459E
        block  ;; label = @3
          block  ;; label = @4
            local.get 4
            i32.load8_u offset=16
            i32.const 3
            i32.ne
            br_if 0 (;@4;)
            local.get 4
            i32.const 0
            i32.store offset=12
            local.get 4
            i32.const 12
            i32.add
            call $_ZN4core3ptr13drop_in_place17h53dd4d82272064b7E
            local.get 4
            i32.const 0
            i32.store offset=8
            local.get 4
            i32.const 8
            i32.add
            call $_ZN4core3ptr13drop_in_place17h53dd4d82272064b7E
            local.get 4
            i32.const 0
            i32.store offset=24
            local.get 4
            i32.const 24
            i32.add
            call $_ZN4core3ptr13drop_in_place17h53dd4d82272064b7E
            local.get 4
            i32.const 0
            i32.store offset=4
            local.get 4
            i32.const 4
            i32.add
            call $_ZN4core3ptr13drop_in_place17h53dd4d82272064b7E
            local.get 0
            i32.load8_u
            i32.const 1
            i32.eq
            br_if 3 (;@1;)
            local.get 0
            i32.load offset=4
            i32.load
            local.tee 1
            local.get 1
            i32.const 8
            i32.add
            local.tee 2
            i32.load
            i32.const 1
            call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E
            local.get 2
            local.get 2
            i32.load
            local.tee 5
            i32.const 1
            i32.add
            i32.store
            local.get 5
            local.get 1
            i32.load
            i32.add
            i32.const 1
            i32.const 1049096
            i32.const 1
            call $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E
            i32.const 0
            local.set 1
            local.get 4
            i32.const 0
            i32.store offset=24
            local.get 4
            i32.const 24
            i32.add
            call $_ZN4core3ptr13drop_in_place17h53dd4d82272064b7E
            local.get 4
            local.get 3
            local.get 0
            i32.load offset=4
            call $_ZN11vector_wasm4role24_IMPL_SERIALIZE_FOR_Role75_$LT$impl$u20$serde..ser..Serialize$u20$for$u20$vector_wasm..role..Role$GT$9serialize17hf6cadadffc6bbaa7E
            local.tee 0
            i32.store offset=24
            block  ;; label = @5
              local.get 0
              br_if 0 (;@5;)
              local.get 4
              i32.const 24
              i32.add
              call $_ZN4core3ptr13drop_in_place17h53dd4d82272064b7E
              local.get 4
              i32.const 0
              i32.store offset=24
              local.get 4
              i32.const 24
              i32.add
              call $_ZN4core3ptr13drop_in_place17h53dd4d82272064b7E
              br 2 (;@3;)
            end
            local.get 0
            local.set 1
            br 1 (;@3;)
          end
          local.get 4
          local.get 4
          i64.load offset=16
          i64.store offset=24
          local.get 4
          i32.const 24
          i32.add
          call $_ZN10serde_json5error5Error2io17h15dd2616b9b8f602E
          local.set 1
        end
        local.get 4
        i32.const 32
        i32.add
        global.set 0
        local.get 1
        return
      end
      i32.const 1048992
      i32.const 40
      i32.const 1049064
      call $_ZN3std9panicking11begin_panic17h8d948eabdf427119E
      unreachable
    end
    i32.const 1048992
    i32.const 40
    i32.const 1049064
    call $_ZN3std9panicking11begin_panic17h8d948eabdf427119E
    unreachable)
  (func $_ZN11vector_wasm12registration12Registration9transform17h71a5e930d73f4337E (type 13) (result i32)
    i32.const 0)
  (func $_ZN11vector_wasm12registration12Registration8register17h6a69957fe30ccf28E (type 0) (param i32)
    local.get 0
    call $_ZN11vector_wasm8hostcall8register17he28f7896e20f1d0aE)
  (func $_ZN11vector_wasm12registration32_IMPL_SERIALIZE_FOR_Registration91_$LT$impl$u20$serde..ser..Serialize$u20$for$u20$vector_wasm..registration..Registration$GT$9serialize17h935ad65576ac8458E (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    local.get 1
    i32.load
    local.tee 3
    local.get 3
    i32.const 8
    i32.add
    local.tee 4
    i32.load
    i32.const 1
    call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E
    local.get 4
    local.get 4
    i32.load
    local.tee 5
    i32.const 1
    i32.add
    i32.store
    local.get 5
    local.get 3
    i32.load
    i32.add
    i32.const 1
    i32.const 1049098
    i32.const 1
    call $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E
    local.get 2
    local.get 1
    i64.extend_i32_u
    i64.const 32
    i64.shl
    i64.const 256
    i64.or
    i64.store offset=8
    block  ;; label = @1
      local.get 2
      i32.const 8
      i32.add
      i32.const 1049099
      i32.const 4
      local.get 0
      call $_ZN5serde3ser12SerializeMap15serialize_entry17h7292117eedface93E
      local.tee 1
      br_if 0 (;@1;)
      local.get 2
      i32.load offset=8
      local.tee 3
      i32.const 255
      i32.and
      i32.const 1
      i32.eq
      br_if 0 (;@1;)
      local.get 3
      i32.const 65280
      i32.and
      i32.eqz
      br_if 0 (;@1;)
      local.get 2
      i32.load offset=12
      i32.load
      local.tee 3
      local.get 3
      i32.const 8
      i32.add
      local.tee 4
      i32.load
      i32.const 1
      call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17he2800ad5c7c510b4E
      local.get 4
      local.get 4
      i32.load
      local.tee 0
      i32.const 1
      i32.add
      i32.store
      local.get 0
      local.get 3
      i32.load
      i32.add
      i32.const 1
      i32.const 1049097
      i32.const 1
      call $_ZN4core5slice29_$LT$impl$u20$$u5b$T$u5d$$GT$15copy_from_slice17h1c7a7387774db3f8E
    end
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN4core3ptr13drop_in_place17ha97f2c661f1fe906E (type 0) (param i32)
    (local i32 i32 i32 i32)
    block  ;; label = @1
      local.get 0
      i32.load
      local.tee 1
      i32.load
      local.tee 2
      i32.const 1
      i32.gt_u
      br_if 0 (;@1;)
      block  ;; label = @2
        block  ;; label = @3
          local.get 2
          br_table 0 (;@3;) 1 (;@2;) 0 (;@3;)
        end
        local.get 1
        i32.const 8
        i32.add
        i32.load
        local.tee 2
        i32.eqz
        br_if 1 (;@1;)
        local.get 1
        i32.load offset=4
        local.get 2
        i32.const 1
        call $__rust_dealloc
        br 1 (;@1;)
      end
      local.get 1
      i32.load8_u offset=4
      i32.const 2
      i32.lt_u
      br_if 0 (;@1;)
      local.get 1
      i32.const 8
      i32.add
      i32.load
      local.tee 2
      i32.load
      local.get 2
      i32.load offset=4
      i32.load
      call_indirect (type 0)
      block  ;; label = @2
        local.get 2
        i32.load offset=4
        local.tee 3
        i32.load offset=4
        local.tee 4
        i32.eqz
        br_if 0 (;@2;)
        local.get 2
        i32.load
        local.get 4
        local.get 3
        i32.load offset=8
        call $__rust_dealloc
      end
      local.get 1
      i32.load offset=8
      i32.const 12
      i32.const 4
      call $__rust_dealloc
    end
    local.get 0
    i32.load
    i32.const 20
    i32.const 4
    call $__rust_dealloc)
  (func $_ZN11vector_wasm8hostcall8register17he28f7896e20f1d0aE (type 0) (param i32)
    (local i32 i32 i32 i64 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 1
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            i32.const 128
            i32.const 1
            call $__rust_alloc
            local.tee 2
            i32.eqz
            br_if 0 (;@4;)
            local.get 1
            i64.const 128
            i64.store offset=4 align=4
            local.get 1
            local.get 2
            i32.store
            local.get 1
            local.get 1
            i32.store offset=12
            local.get 0
            local.get 1
            i32.const 12
            i32.add
            call $_ZN11vector_wasm12registration32_IMPL_SERIALIZE_FOR_Registration91_$LT$impl$u20$serde..ser..Serialize$u20$for$u20$vector_wasm..registration..Registration$GT$9serialize17h935ad65576ac8458E
            local.tee 0
            br_if 1 (;@3;)
            local.get 1
            i32.load
            local.set 3
            block  ;; label = @5
              block  ;; label = @6
                local.get 1
                i64.load offset=4 align=4
                local.tee 4
                i32.wrap_i64
                local.tee 5
                local.get 4
                i64.const 32
                i64.shr_u
                i32.wrap_i64
                local.tee 0
                i32.ne
                br_if 0 (;@6;)
                local.get 3
                local.set 2
                local.get 5
                local.set 0
                br 1 (;@5;)
              end
              local.get 5
              local.get 0
              i32.lt_u
              br_if 3 (;@2;)
              block  ;; label = @6
                local.get 0
                br_if 0 (;@6;)
                i32.const 0
                local.set 0
                i32.const 1
                local.set 2
                local.get 5
                i32.eqz
                br_if 1 (;@5;)
                local.get 3
                local.get 5
                i32.const 1
                call $__rust_dealloc
                br 1 (;@5;)
              end
              local.get 3
              local.get 5
              i32.const 1
              local.get 0
              call $__rust_realloc
              local.tee 2
              i32.eqz
              br_if 4 (;@1;)
            end
            local.get 2
            i64.extend_i32_u
            local.get 0
            i64.extend_i32_u
            call $register
            block  ;; label = @5
              local.get 0
              i32.eqz
              br_if 0 (;@5;)
              local.get 2
              local.get 0
              i32.const 1
              call $__rust_dealloc
            end
            local.get 1
            i32.const 16
            i32.add
            global.set 0
            return
          end
          i32.const 128
          i32.const 1
          call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
          unreachable
        end
        local.get 1
        call $_ZN77_$LT$alloc..raw_vec..RawVec$LT$T$C$A$GT$$u20$as$u20$core..ops..drop..Drop$GT$4drop17hf042fdaaf184435fE
        local.get 1
        local.get 0
        i32.store
        i32.const 1049103
        i32.const 43
        local.get 1
        i32.const 1049148
        i32.const 1049196
        call $_ZN4core6option18expect_none_failed17h5718e8afd751d0acE
        unreachable
      end
      i32.const 1048672
      i32.const 36
      i32.const 1048740
      call $_ZN4core9panicking5panic17he9463ceb3e2615beE
      unreachable
    end
    local.get 0
    i32.const 1
    call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
    unreachable)
  (func $_ZN11vector_wasm4role24_IMPL_SERIALIZE_FOR_Role75_$LT$impl$u20$serde..ser..Serialize$u20$for$u20$vector_wasm..role..Role$GT$9serialize17hf6cadadffc6bbaa7E (type 2) (param i32 i32) (result i32)
    (local i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 0
            i32.load
            br_table 1 (;@3;) 2 (;@2;) 0 (;@4;) 1 (;@3;)
          end
          local.get 2
          local.get 1
          local.get 1
          i32.const 1049212
          i32.const 4
          call $_ZN10serde_json3ser18format_escaped_str17h909b592d21f22459E
          i32.const 0
          local.set 1
          local.get 2
          i32.load8_u
          i32.const 3
          i32.eq
          br_if 2 (;@1;)
          local.get 2
          local.get 2
          i64.load
          i64.store offset=8
          local.get 2
          i32.const 8
          i32.add
          call $_ZN10serde_json5error5Error2io17h15dd2616b9b8f602E
          local.set 1
          br 2 (;@1;)
        end
        local.get 2
        local.get 1
        local.get 1
        i32.const 1049222
        i32.const 9
        call $_ZN10serde_json3ser18format_escaped_str17h909b592d21f22459E
        i32.const 0
        local.set 1
        local.get 2
        i32.load8_u
        i32.const 3
        i32.eq
        br_if 1 (;@1;)
        local.get 2
        local.get 2
        i64.load
        i64.store offset=8
        local.get 2
        i32.const 8
        i32.add
        call $_ZN10serde_json5error5Error2io17h15dd2616b9b8f602E
        local.set 1
        br 1 (;@1;)
      end
      local.get 2
      local.get 1
      local.get 1
      i32.const 1049216
      i32.const 6
      call $_ZN10serde_json3ser18format_escaped_str17h909b592d21f22459E
      i32.const 0
      local.set 1
      local.get 2
      i32.load8_u
      i32.const 3
      i32.eq
      br_if 0 (;@1;)
      local.get 2
      local.get 2
      i64.load
      i64.store offset=8
      local.get 2
      i32.const 8
      i32.add
      call $_ZN10serde_json5error5Error2io17h15dd2616b9b8f602E
      local.set 1
    end
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h18a30a599fe658f8E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.set 0
    block  ;; label = @1
      local.get 1
      call $_ZN4core3fmt9Formatter15debug_lower_hex17h56f7e617e2f0ca72E
      br_if 0 (;@1;)
      block  ;; label = @2
        local.get 1
        call $_ZN4core3fmt9Formatter15debug_upper_hex17h009e81a991e324e6E
        br_if 0 (;@2;)
        local.get 0
        local.get 1
        call $_ZN4core3fmt3num3imp52_$LT$impl$u20$core..fmt..Display$u20$for$u20$u32$GT$3fmt17h976c74654a4bcc54E
        return
      end
      local.get 0
      local.get 1
      call $_ZN4core3fmt3num53_$LT$impl$u20$core..fmt..UpperHex$u20$for$u20$i32$GT$3fmt17hb943c22d3c4cfecbE
      return
    end
    local.get 0
    local.get 1
    call $_ZN4core3fmt3num53_$LT$impl$u20$core..fmt..LowerHex$u20$for$u20$i32$GT$3fmt17h957181898b1e70adE)
  (func $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17h6a14ba090fa87b57E (type 1) (param i32) (result i64)
    i64.const 1229646359891580772)
  (func $_ZN3std9panicking11begin_panic17h8d948eabdf427119E (type 5) (param i32 i32 i32)
    (local i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    local.get 1
    i32.store offset=12
    local.get 3
    local.get 0
    i32.store offset=8
    local.get 3
    i32.const 8
    i32.add
    i32.const 1049232
    i32.const 0
    local.get 2
    call $_ZN4core5panic8Location6caller17hba7ec45f0d210bdeE
    call $_ZN3std9panicking20rust_panic_with_hook17h8bf13b9f643a54b1E
    unreachable)
  (func $_ZN4core3ptr13drop_in_place17h79d74b610ac14d1fE (type 0) (param i32))
  (func $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$3get17h2d6887d8afeafa81E (type 4) (param i32 i32)
    block  ;; label = @1
      local.get 1
      i32.load
      br_if 0 (;@1;)
      call $_ZN3std7process5abort17h1646aa60de17f512E
      unreachable
    end
    local.get 0
    i32.const 1049252
    i32.store offset=4
    local.get 0
    local.get 1
    i32.store)
  (func $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$8take_box17h7d082bbcb9b85d03E (type 4) (param i32 i32)
    (local i32 i32)
    local.get 1
    i32.load
    local.set 2
    local.get 1
    i32.const 0
    i32.store
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.eqz
        br_if 0 (;@2;)
        local.get 1
        i32.load offset=4
        local.set 3
        i32.const 8
        i32.const 4
        call $__rust_alloc
        local.tee 1
        i32.eqz
        br_if 1 (;@1;)
        local.get 1
        local.get 3
        i32.store offset=4
        local.get 1
        local.get 2
        i32.store
        local.get 0
        i32.const 1049252
        i32.store offset=4
        local.get 0
        local.get 1
        i32.store
        return
      end
      call $_ZN3std7process5abort17h1646aa60de17f512E
      unreachable
    end
    i32.const 8
    i32.const 4
    call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
    unreachable)
  (func $_ZN4core3ptr13drop_in_place17hbf7e19099f11a74dE (type 0) (param i32))
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$10write_char17hf74f26a32dd33facE (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    local.get 0
    i32.load
    local.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 1
            i32.const 128
            i32.lt_u
            br_if 0 (;@4;)
            local.get 2
            i32.const 0
            i32.store offset=12
            local.get 1
            i32.const 2048
            i32.lt_u
            br_if 1 (;@3;)
            block  ;; label = @5
              local.get 1
              i32.const 65536
              i32.ge_u
              br_if 0 (;@5;)
              local.get 2
              local.get 1
              i32.const 63
              i32.and
              i32.const 128
              i32.or
              i32.store8 offset=14
              local.get 2
              local.get 1
              i32.const 6
              i32.shr_u
              i32.const 63
              i32.and
              i32.const 128
              i32.or
              i32.store8 offset=13
              local.get 2
              local.get 1
              i32.const 12
              i32.shr_u
              i32.const 15
              i32.and
              i32.const 224
              i32.or
              i32.store8 offset=12
              i32.const 3
              local.set 1
              br 3 (;@2;)
            end
            local.get 2
            local.get 1
            i32.const 63
            i32.and
            i32.const 128
            i32.or
            i32.store8 offset=15
            local.get 2
            local.get 1
            i32.const 18
            i32.shr_u
            i32.const 240
            i32.or
            i32.store8 offset=12
            local.get 2
            local.get 1
            i32.const 6
            i32.shr_u
            i32.const 63
            i32.and
            i32.const 128
            i32.or
            i32.store8 offset=14
            local.get 2
            local.get 1
            i32.const 12
            i32.shr_u
            i32.const 63
            i32.and
            i32.const 128
            i32.or
            i32.store8 offset=13
            i32.const 4
            local.set 1
            br 2 (;@2;)
          end
          block  ;; label = @4
            local.get 0
            i32.load offset=8
            local.tee 3
            local.get 0
            i32.const 4
            i32.add
            i32.load
            i32.ne
            br_if 0 (;@4;)
            local.get 0
            local.get 3
            i32.const 1
            call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17h1a9123d1a43338eaE
            local.get 0
            i32.load offset=8
            local.set 3
          end
          local.get 0
          i32.load
          local.get 3
          i32.add
          local.get 1
          i32.store8
          local.get 0
          local.get 0
          i32.load offset=8
          i32.const 1
          i32.add
          i32.store offset=8
          br 2 (;@1;)
        end
        local.get 2
        local.get 1
        i32.const 63
        i32.and
        i32.const 128
        i32.or
        i32.store8 offset=13
        local.get 2
        local.get 1
        i32.const 6
        i32.shr_u
        i32.const 31
        i32.and
        i32.const 192
        i32.or
        i32.store8 offset=12
        i32.const 2
        local.set 1
      end
      local.get 0
      local.get 0
      i32.const 8
      i32.add
      local.tee 3
      i32.load
      local.get 1
      call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17h1a9123d1a43338eaE
      local.get 3
      local.get 3
      i32.load
      local.tee 4
      local.get 1
      i32.add
      i32.store
      local.get 4
      local.get 0
      i32.load
      i32.add
      local.get 2
      i32.const 12
      i32.add
      local.get 1
      call $memcpy
      drop
    end
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    i32.const 0)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_fmt17h0da91fd8f8378a91E (type 2) (param i32 i32) (result i32)
    (local i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 0
    i32.load
    i32.store offset=4
    local.get 2
    i32.const 8
    i32.add
    i32.const 16
    i32.add
    local.get 1
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    i32.const 8
    i32.add
    i32.const 8
    i32.add
    local.get 1
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    local.get 1
    i64.load align=4
    i64.store offset=8
    local.get 2
    i32.const 4
    i32.add
    i32.const 1049268
    local.get 2
    i32.const 8
    i32.add
    call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
    local.set 1
    local.get 2
    i32.const 32
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_str17h50c0b4ca9feb25fcE (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32)
    local.get 0
    i32.load
    local.tee 0
    local.get 0
    i32.const 8
    i32.add
    local.tee 3
    i32.load
    local.get 2
    call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17h1a9123d1a43338eaE
    local.get 3
    local.get 3
    i32.load
    local.tee 4
    local.get 2
    i32.add
    i32.store
    local.get 4
    local.get 0
    i32.load
    i32.add
    local.get 1
    local.get 2
    call $memcpy
    drop
    i32.const 0)
  (func $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$7reserve17h1a9123d1a43338eaE (type 5) (param i32 i32 i32)
    (local i32)
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 0
          i32.const 4
          i32.add
          i32.load
          local.tee 3
          local.get 1
          i32.sub
          local.get 2
          i32.ge_u
          br_if 0 (;@3;)
          local.get 1
          local.get 2
          i32.add
          local.tee 2
          local.get 1
          i32.lt_u
          br_if 2 (;@1;)
          local.get 3
          i32.const 1
          i32.shl
          local.tee 1
          local.get 2
          local.get 1
          local.get 2
          i32.gt_u
          select
          local.tee 1
          i32.const 0
          i32.lt_s
          br_if 2 (;@1;)
          block  ;; label = @4
            block  ;; label = @5
              local.get 3
              br_if 0 (;@5;)
              local.get 1
              i32.const 1
              call $__rust_alloc
              local.set 2
              br 1 (;@4;)
            end
            local.get 0
            i32.load
            local.get 3
            i32.const 1
            local.get 1
            call $__rust_realloc
            local.set 2
          end
          local.get 2
          i32.eqz
          br_if 1 (;@2;)
          local.get 0
          local.get 2
          i32.store
          local.get 0
          i32.const 4
          i32.add
          local.get 1
          i32.store
        end
        return
      end
      local.get 1
      i32.const 1
      call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
      unreachable
    end
    call $_ZN5alloc7raw_vec17capacity_overflow17h60fd539dfca5134dE
    unreachable)
  (func $_ZN44_$LT$$RF$T$u20$as$u20$core..fmt..Display$GT$3fmt17h06b8d40f9bff9871E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 1
    call $_ZN67_$LT$serde_json..error..ErrorCode$u20$as$u20$core..fmt..Display$GT$3fmt17ha9dd79b4056b50e6E)
  (func $_ZN67_$LT$serde_json..error..ErrorCode$u20$as$u20$core..fmt..Display$GT$3fmt17ha9dd79b4056b50e6E (type 2) (param i32 i32) (result i32)
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      block  ;; label = @10
                        block  ;; label = @11
                          block  ;; label = @12
                            block  ;; label = @13
                              block  ;; label = @14
                                block  ;; label = @15
                                  block  ;; label = @16
                                    block  ;; label = @17
                                      block  ;; label = @18
                                        block  ;; label = @19
                                          block  ;; label = @20
                                            block  ;; label = @21
                                              block  ;; label = @22
                                                local.get 0
                                                i32.load
                                                br_table 1 (;@21;) 2 (;@20;) 3 (;@19;) 4 (;@18;) 5 (;@17;) 6 (;@16;) 7 (;@15;) 8 (;@14;) 9 (;@13;) 10 (;@12;) 11 (;@11;) 12 (;@10;) 13 (;@9;) 14 (;@8;) 15 (;@7;) 16 (;@6;) 17 (;@5;) 18 (;@4;) 19 (;@3;) 20 (;@2;) 21 (;@1;) 0 (;@22;) 1 (;@21;)
                                              end
                                              local.get 1
                                              i32.const 1049816
                                              i32.const 24
                                              call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                                              return
                                            end
                                            local.get 1
                                            local.get 0
                                            i32.load offset=4
                                            local.get 0
                                            i32.const 8
                                            i32.add
                                            i32.load
                                            call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                                            return
                                          end
                                          local.get 0
                                          i32.const 4
                                          i32.add
                                          local.get 1
                                          call $_ZN60_$LT$std..io..error..Error$u20$as$u20$core..fmt..Display$GT$3fmt17hfbf3380c56bbc0baE
                                          return
                                        end
                                        local.get 1
                                        i32.const 1050248
                                        i32.const 24
                                        call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                                        return
                                      end
                                      local.get 1
                                      i32.const 1050221
                                      i32.const 27
                                      call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                                      return
                                    end
                                    local.get 1
                                    i32.const 1050195
                                    i32.const 26
                                    call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                                    return
                                  end
                                  local.get 1
                                  i32.const 1050170
                                  i32.const 25
                                  call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                                  return
                                end
                                local.get 1
                                i32.const 1050158
                                i32.const 12
                                call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                                return
                              end
                              local.get 1
                              i32.const 1050139
                              i32.const 19
                              call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                              return
                            end
                            local.get 1
                            i32.const 1050120
                            i32.const 19
                            call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                            return
                          end
                          local.get 1
                          i32.const 1050106
                          i32.const 14
                          call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                          return
                        end
                        local.get 1
                        i32.const 1050092
                        i32.const 14
                        call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                        return
                      end
                      local.get 1
                      i32.const 1050078
                      i32.const 14
                      call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                      return
                    end
                    local.get 1
                    i32.const 1050064
                    i32.const 14
                    call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                    return
                  end
                  local.get 1
                  i32.const 1050045
                  i32.const 19
                  call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                  return
                end
                local.get 1
                i32.const 1050019
                i32.const 26
                call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
                return
              end
              local.get 1
              i32.const 1049957
              i32.const 62
              call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
              return
            end
            local.get 1
            i32.const 1049937
            i32.const 20
            call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
            return
          end
          local.get 1
          i32.const 1049901
          i32.const 36
          call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
          return
        end
        local.get 1
        i32.const 1049887
        i32.const 14
        call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
        return
      end
      local.get 1
      i32.const 1049868
      i32.const 19
      call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
      return
    end
    local.get 1
    i32.const 1049840
    i32.const 28
    call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E)
  (func $_ZN4core3ptr13drop_in_place17h93127de8e088b4c0E (type 0) (param i32))
  (func $_ZN58_$LT$alloc..string..String$u20$as$u20$core..fmt..Debug$GT$3fmt17hb1335b498f38353fE (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 0
    i32.load offset=8
    local.get 1
    call $_ZN40_$LT$str$u20$as$u20$core..fmt..Debug$GT$3fmt17h1a54fd8ecae06fe9E)
  (func $_ZN10serde_json5error5Error2io17h15dd2616b9b8f602E (type 14) (param i32) (result i32)
    (local i64)
    local.get 0
    i64.load align=4
    local.set 1
    block  ;; label = @1
      i32.const 20
      i32.const 4
      call $__rust_alloc
      local.tee 0
      br_if 0 (;@1;)
      i32.const 20
      i32.const 4
      call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
      unreachable
    end
    local.get 0
    i64.const 0
    i64.store offset=12 align=4
    local.get 0
    local.get 1
    i64.store offset=4 align=4
    local.get 0
    i32.const 1
    i32.store
    local.get 0)
  (func $_ZN61_$LT$serde_json..error..Error$u20$as$u20$core..fmt..Debug$GT$3fmt17h7e3fb3b68632231bE (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 80
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 0
    i32.load
    local.tee 0
    i32.store offset=36
    local.get 2
    i32.const 0
    i32.store offset=48
    local.get 2
    i64.const 1
    i64.store offset=40
    local.get 2
    i32.const 17
    i32.store offset=4
    local.get 2
    local.get 2
    i32.const 36
    i32.add
    i32.store
    local.get 2
    local.get 2
    i32.const 40
    i32.add
    i32.store offset=24
    local.get 2
    i32.const 76
    i32.add
    i32.const 1
    i32.store
    local.get 2
    i64.const 1
    i64.store offset=60 align=4
    local.get 2
    i32.const 1049648
    i32.store offset=56
    local.get 2
    local.get 2
    i32.store offset=72
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 2
          i32.const 24
          i32.add
          i32.const 1049268
          local.get 2
          i32.const 56
          i32.add
          call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
          br_if 0 (;@3;)
          block  ;; label = @4
            local.get 2
            i32.load offset=44
            local.tee 3
            local.get 2
            i32.load offset=48
            local.tee 4
            i32.eq
            br_if 0 (;@4;)
            local.get 3
            local.get 4
            i32.lt_u
            br_if 2 (;@2;)
            block  ;; label = @5
              block  ;; label = @6
                local.get 4
                br_if 0 (;@6;)
                block  ;; label = @7
                  local.get 3
                  i32.eqz
                  br_if 0 (;@7;)
                  local.get 2
                  i32.load offset=40
                  local.get 3
                  i32.const 1
                  call $__rust_dealloc
                end
                local.get 2
                i32.const 1
                i32.store offset=40
                i32.const 0
                local.set 4
                br 1 (;@5;)
              end
              local.get 2
              i32.load offset=40
              local.get 3
              i32.const 1
              local.get 4
              call $__rust_realloc
              local.tee 3
              i32.eqz
              br_if 4 (;@1;)
              local.get 2
              local.get 3
              i32.store offset=40
            end
            local.get 2
            local.get 4
            i32.store offset=44
          end
          local.get 2
          i32.const 24
          i32.add
          i32.const 8
          i32.add
          local.get 2
          i32.const 40
          i32.add
          i32.const 8
          i32.add
          i32.load
          i32.store
          local.get 2
          i32.const 56
          i32.add
          i32.const 20
          i32.add
          i32.const 18
          i32.store
          local.get 2
          i32.const 56
          i32.add
          i32.const 12
          i32.add
          i32.const 18
          i32.store
          local.get 2
          i32.const 20
          i32.add
          i32.const 3
          i32.store
          local.get 2
          local.get 2
          i64.load offset=40
          i64.store offset=24
          local.get 2
          i32.const 19
          i32.store offset=60
          local.get 2
          i64.const 4
          i64.store offset=4 align=4
          local.get 2
          i32.const 1050300
          i32.store
          local.get 2
          local.get 0
          i32.const 16
          i32.add
          i32.store offset=72
          local.get 2
          local.get 0
          i32.const 12
          i32.add
          i32.store offset=64
          local.get 2
          local.get 2
          i32.const 24
          i32.add
          i32.store offset=56
          local.get 2
          local.get 2
          i32.const 56
          i32.add
          i32.store offset=16
          local.get 1
          local.get 2
          call $_ZN4core3fmt9Formatter9write_fmt17ha552aa6bb1a0a03bE
          local.set 0
          block  ;; label = @4
            local.get 2
            i32.load offset=28
            local.tee 4
            i32.eqz
            br_if 0 (;@4;)
            local.get 2
            i32.load offset=24
            local.get 4
            i32.const 1
            call $__rust_dealloc
          end
          local.get 2
          i32.const 80
          i32.add
          global.set 0
          local.get 0
          return
        end
        i32.const 1049656
        i32.const 55
        local.get 2
        i32.const 56
        i32.add
        i32.const 1049800
        i32.const 1049784
        call $_ZN4core6option18expect_none_failed17h5718e8afd751d0acE
        unreachable
      end
      i32.const 1049564
      i32.const 36
      i32.const 1049632
      call $_ZN4core9panicking5panic17he9463ceb3e2615beE
      unreachable
    end
    local.get 4
    i32.const 1
    call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
    unreachable)
  (func $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17h310bf071aa6f797cE (type 1) (param i32) (result i64)
    i64.const 1326782401342502364)
  (func $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17h6f2f3a966973ed98E (type 1) (param i32) (result i64)
    i64.const 1229646359891580772)
  (func $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17hcde11046253965a8E (type 1) (param i32) (result i64)
    i64.const 794850088668468598)
  (func $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h316e40b6c6984df5E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.set 0
    block  ;; label = @1
      local.get 1
      call $_ZN4core3fmt9Formatter15debug_lower_hex17h56f7e617e2f0ca72E
      br_if 0 (;@1;)
      block  ;; label = @2
        local.get 1
        call $_ZN4core3fmt9Formatter15debug_upper_hex17h009e81a991e324e6E
        br_if 0 (;@2;)
        local.get 0
        local.get 1
        call $_ZN4core3fmt3num3imp52_$LT$impl$u20$core..fmt..Display$u20$for$u20$u32$GT$3fmt17h976c74654a4bcc54E
        return
      end
      local.get 0
      local.get 1
      call $_ZN4core3fmt3num53_$LT$impl$u20$core..fmt..UpperHex$u20$for$u20$i32$GT$3fmt17hb943c22d3c4cfecbE
      return
    end
    local.get 0
    local.get 1
    call $_ZN4core3fmt3num53_$LT$impl$u20$core..fmt..LowerHex$u20$for$u20$i32$GT$3fmt17h957181898b1e70adE)
  (func $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h595c62cfe8d58551E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.set 0
    block  ;; label = @1
      local.get 1
      call $_ZN4core3fmt9Formatter15debug_lower_hex17h56f7e617e2f0ca72E
      br_if 0 (;@1;)
      block  ;; label = @2
        local.get 1
        call $_ZN4core3fmt9Formatter15debug_upper_hex17h009e81a991e324e6E
        br_if 0 (;@2;)
        local.get 0
        local.get 1
        call $_ZN4core3fmt3num3imp51_$LT$impl$u20$core..fmt..Display$u20$for$u20$u8$GT$3fmt17h041f3dd69513682bE
        return
      end
      local.get 0
      local.get 1
      call $_ZN4core3fmt3num52_$LT$impl$u20$core..fmt..UpperHex$u20$for$u20$i8$GT$3fmt17hebc0280df4365f65E
      return
    end
    local.get 0
    local.get 1
    call $_ZN4core3fmt3num52_$LT$impl$u20$core..fmt..LowerHex$u20$for$u20$i8$GT$3fmt17hee01ea12b036c189E)
  (func $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h61c13854410d74b4E (type 2) (param i32 i32) (result i32)
    (local i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    local.get 0
    i32.load
    local.tee 0
    i32.load offset=8
    local.set 3
    local.get 0
    i32.load
    local.set 0
    local.get 2
    local.get 1
    call $_ZN4core3fmt9Formatter10debug_list17h1d7285676248dd4eE
    block  ;; label = @1
      local.get 3
      i32.eqz
      br_if 0 (;@1;)
      loop  ;; label = @2
        local.get 2
        local.get 0
        i32.store offset=12
        local.get 2
        local.get 2
        i32.const 12
        i32.add
        i32.const 1050380
        call $_ZN4core3fmt8builders8DebugSet5entry17ha23dddb04336d96cE
        drop
        local.get 0
        i32.const 1
        i32.add
        local.set 0
        local.get 3
        i32.const -1
        i32.add
        local.tee 3
        br_if 0 (;@2;)
      end
    end
    local.get 2
    call $_ZN4core3fmt8builders9DebugList6finish17h6732b39c5e331c0aE
    local.set 0
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN73_$LT$std..sys_common..os_str_bytes..Slice$u20$as$u20$core..fmt..Debug$GT$3fmt17h8b2c8a7213186b17E (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32 i32 i32 i64)
    global.get 0
    i32.const 80
    i32.sub
    local.tee 3
    global.set 0
    i32.const 1
    local.set 4
    block  ;; label = @1
      local.get 2
      i32.const 1051124
      i32.const 1
      call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
      br_if 0 (;@1;)
      local.get 3
      i32.const 8
      i32.add
      local.get 0
      local.get 1
      call $_ZN4core3str5lossy9Utf8Lossy10from_bytes17h1357f46792efee29E
      local.get 3
      local.get 3
      i32.load offset=8
      local.get 3
      i32.load offset=12
      call $_ZN4core3str5lossy9Utf8Lossy6chunks17hc5734690a50ae00eE
      local.get 3
      local.get 3
      i64.load
      i64.store offset=16
      local.get 3
      i32.const 40
      i32.add
      local.get 3
      i32.const 16
      i32.add
      call $_ZN96_$LT$core..str..lossy..Utf8LossyChunksIter$u20$as$u20$core..iter..traits..iterator..Iterator$GT$4next17hdd79ec53ab0551bdE
      block  ;; label = @2
        local.get 3
        i32.load offset=40
        local.tee 4
        i32.eqz
        br_if 0 (;@2;)
        local.get 3
        i32.const 48
        i32.add
        local.set 5
        local.get 3
        i32.const 64
        i32.add
        local.set 6
        loop  ;; label = @3
          local.get 3
          i32.load offset=52
          local.set 7
          local.get 3
          i32.load offset=48
          local.set 8
          local.get 3
          i32.load offset=44
          local.set 0
          local.get 3
          i32.const 4
          i32.store offset=64
          local.get 3
          i32.const 4
          i32.store offset=48
          local.get 3
          local.get 4
          i32.store offset=40
          local.get 3
          local.get 4
          local.get 0
          i32.add
          i32.store offset=44
          i32.const 4
          local.set 4
          block  ;; label = @4
            loop  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      block  ;; label = @10
                        block  ;; label = @11
                          block  ;; label = @12
                            block  ;; label = @13
                              block  ;; label = @14
                                block  ;; label = @15
                                  block  ;; label = @16
                                    block  ;; label = @17
                                      local.get 4
                                      i32.const 4
                                      i32.eq
                                      br_if 0 (;@17;)
                                      local.get 5
                                      call $_ZN82_$LT$core..char..EscapeDebug$u20$as$u20$core..iter..traits..iterator..Iterator$GT$4next17h8d8e9bdd03c8beb6E
                                      local.tee 4
                                      i32.const 1114112
                                      i32.ne
                                      br_if 1 (;@16;)
                                    end
                                    block  ;; label = @17
                                      local.get 3
                                      i32.load offset=40
                                      local.tee 4
                                      local.get 3
                                      i32.load offset=44
                                      local.tee 0
                                      i32.eq
                                      br_if 0 (;@17;)
                                      local.get 3
                                      local.get 4
                                      i32.const 1
                                      i32.add
                                      local.tee 9
                                      i32.store offset=40
                                      block  ;; label = @18
                                        block  ;; label = @19
                                          local.get 4
                                          i32.load8_s
                                          local.tee 1
                                          i32.const -1
                                          i32.le_s
                                          br_if 0 (;@19;)
                                          local.get 1
                                          i32.const 255
                                          i32.and
                                          local.set 0
                                          br 1 (;@18;)
                                        end
                                        block  ;; label = @19
                                          block  ;; label = @20
                                            local.get 9
                                            local.get 0
                                            i32.ne
                                            br_if 0 (;@20;)
                                            i32.const 0
                                            local.set 4
                                            local.get 0
                                            local.set 9
                                            br 1 (;@19;)
                                          end
                                          local.get 3
                                          local.get 4
                                          i32.const 2
                                          i32.add
                                          local.tee 9
                                          i32.store offset=40
                                          local.get 4
                                          i32.load8_u offset=1
                                          i32.const 63
                                          i32.and
                                          local.set 4
                                        end
                                        local.get 1
                                        i32.const 31
                                        i32.and
                                        local.set 10
                                        block  ;; label = @19
                                          local.get 1
                                          i32.const 255
                                          i32.and
                                          local.tee 1
                                          i32.const 223
                                          i32.gt_u
                                          br_if 0 (;@19;)
                                          local.get 4
                                          local.get 10
                                          i32.const 6
                                          i32.shl
                                          i32.or
                                          local.set 0
                                          br 1 (;@18;)
                                        end
                                        block  ;; label = @19
                                          block  ;; label = @20
                                            local.get 9
                                            local.get 0
                                            i32.ne
                                            br_if 0 (;@20;)
                                            i32.const 0
                                            local.set 9
                                            local.get 0
                                            local.set 11
                                            br 1 (;@19;)
                                          end
                                          local.get 3
                                          local.get 9
                                          i32.const 1
                                          i32.add
                                          local.tee 11
                                          i32.store offset=40
                                          local.get 9
                                          i32.load8_u
                                          i32.const 63
                                          i32.and
                                          local.set 9
                                        end
                                        local.get 9
                                        local.get 4
                                        i32.const 6
                                        i32.shl
                                        i32.or
                                        local.set 4
                                        block  ;; label = @19
                                          local.get 1
                                          i32.const 240
                                          i32.ge_u
                                          br_if 0 (;@19;)
                                          local.get 4
                                          local.get 10
                                          i32.const 12
                                          i32.shl
                                          i32.or
                                          local.set 0
                                          br 1 (;@18;)
                                        end
                                        block  ;; label = @19
                                          block  ;; label = @20
                                            local.get 11
                                            local.get 0
                                            i32.ne
                                            br_if 0 (;@20;)
                                            i32.const 0
                                            local.set 0
                                            br 1 (;@19;)
                                          end
                                          local.get 3
                                          local.get 11
                                          i32.const 1
                                          i32.add
                                          i32.store offset=40
                                          local.get 11
                                          i32.load8_u
                                          i32.const 63
                                          i32.and
                                          local.set 0
                                        end
                                        local.get 4
                                        i32.const 6
                                        i32.shl
                                        local.get 10
                                        i32.const 18
                                        i32.shl
                                        i32.const 1835008
                                        i32.and
                                        i32.or
                                        local.get 0
                                        i32.or
                                        local.set 0
                                      end
                                      i32.const 2
                                      local.set 4
                                      local.get 0
                                      i32.const -9
                                      i32.add
                                      local.tee 9
                                      i32.const 30
                                      i32.le_u
                                      br_if 4 (;@13;)
                                      local.get 0
                                      i32.const 92
                                      i32.eq
                                      br_if 6 (;@11;)
                                      local.get 0
                                      i32.const 1114112
                                      i32.ne
                                      br_if 5 (;@12;)
                                    end
                                    local.get 3
                                    i32.load offset=64
                                    i32.const 4
                                    i32.eq
                                    br_if 1 (;@15;)
                                    local.get 6
                                    call $_ZN82_$LT$core..char..EscapeDebug$u20$as$u20$core..iter..traits..iterator..Iterator$GT$4next17h8d8e9bdd03c8beb6E
                                    local.tee 4
                                    i32.const 1114112
                                    i32.eq
                                    br_if 1 (;@15;)
                                  end
                                  local.get 2
                                  local.get 4
                                  call $_ZN57_$LT$core..fmt..Formatter$u20$as$u20$core..fmt..Write$GT$10write_char17ha4e8098fcbda3807E
                                  br_if 1 (;@14;)
                                  local.get 3
                                  i32.load offset=48
                                  local.set 4
                                  br 10 (;@5;)
                                end
                                loop  ;; label = @15
                                  local.get 7
                                  i32.eqz
                                  br_if 11 (;@4;)
                                  local.get 3
                                  local.get 8
                                  i32.store offset=28
                                  local.get 3
                                  i32.const 1
                                  i32.store offset=60
                                  local.get 3
                                  i32.const 1
                                  i32.store offset=52
                                  local.get 3
                                  i32.const 1051708
                                  i32.store offset=48
                                  local.get 3
                                  i32.const 1
                                  i32.store offset=44
                                  local.get 3
                                  i32.const 1051700
                                  i32.store offset=40
                                  local.get 3
                                  i32.const 22
                                  i32.store offset=36
                                  local.get 7
                                  i32.const -1
                                  i32.add
                                  local.set 7
                                  local.get 8
                                  i32.const 1
                                  i32.add
                                  local.set 8
                                  local.get 3
                                  local.get 3
                                  i32.const 32
                                  i32.add
                                  i32.store offset=56
                                  local.get 3
                                  local.get 3
                                  i32.const 28
                                  i32.add
                                  i32.store offset=32
                                  local.get 2
                                  local.get 3
                                  i32.const 40
                                  i32.add
                                  call $_ZN4core3fmt9Formatter9write_fmt17ha552aa6bb1a0a03bE
                                  i32.eqz
                                  br_if 0 (;@15;)
                                end
                              end
                              i32.const 1
                              local.set 4
                              br 12 (;@1;)
                            end
                            i32.const 116
                            local.set 1
                            block  ;; label = @13
                              local.get 9
                              br_table 7 (;@6;) 5 (;@8;) 1 (;@12;) 1 (;@12;) 0 (;@13;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 2 (;@11;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 1 (;@12;) 2 (;@11;) 7 (;@6;)
                            end
                            i32.const 114
                            local.set 1
                            br 5 (;@7;)
                          end
                          block  ;; label = @12
                            local.get 0
                            call $_ZN4core7unicode12unicode_data15grapheme_extend6lookup17he32874b852959152E
                            i32.eqz
                            br_if 0 (;@12;)
                            local.get 0
                            i32.const 1
                            i32.or
                            i32.clz
                            i32.const 2
                            i32.shr_u
                            i32.const 7
                            i32.xor
                            i64.extend_i32_u
                            i64.const 21474836480
                            i64.or
                            local.set 12
                            br 3 (;@9;)
                          end
                          i32.const 1
                          local.set 4
                          local.get 0
                          call $_ZN4core7unicode9printable12is_printable17h28b8beadd74ff247E
                          i32.eqz
                          br_if 1 (;@10;)
                        end
                        local.get 0
                        local.set 1
                        br 4 (;@6;)
                      end
                      local.get 0
                      i32.const 1
                      i32.or
                      i32.clz
                      i32.const 2
                      i32.shr_u
                      i32.const 7
                      i32.xor
                      i64.extend_i32_u
                      i64.const 21474836480
                      i64.or
                      local.set 12
                    end
                    i32.const 3
                    local.set 4
                    local.get 0
                    local.set 1
                    br 2 (;@6;)
                  end
                  i32.const 110
                  local.set 1
                end
              end
              local.get 3
              local.get 12
              i64.store offset=56
              local.get 3
              local.get 1
              i32.store offset=52
              local.get 3
              local.get 4
              i32.store offset=48
              br 0 (;@5;)
            end
          end
          local.get 3
          i32.const 40
          i32.add
          local.get 3
          i32.const 16
          i32.add
          call $_ZN96_$LT$core..str..lossy..Utf8LossyChunksIter$u20$as$u20$core..iter..traits..iterator..Iterator$GT$4next17hdd79ec53ab0551bdE
          local.get 3
          i32.load offset=40
          local.tee 4
          br_if 0 (;@3;)
        end
      end
      local.get 2
      i32.const 1051124
      i32.const 1
      call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E
      local.set 4
    end
    local.get 3
    i32.const 80
    i32.add
    global.set 0
    local.get 4)
  (func $_ZN44_$LT$$RF$T$u20$as$u20$core..fmt..Display$GT$3fmt17h0488b928e863645bE (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 0
    i32.load offset=4
    local.get 1
    call $_ZN42_$LT$str$u20$as$u20$core..fmt..Display$GT$3fmt17hcf977a4d08f25cc0E)
  (func $_ZN44_$LT$$RF$T$u20$as$u20$core..fmt..Display$GT$3fmt17hb3c88bf73f5dda2eE (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 1
    call $_ZN60_$LT$core..panic..Location$u20$as$u20$core..fmt..Display$GT$3fmt17h79c3ef5172e6604aE)
  (func $_ZN45_$LT$$RF$T$u20$as$u20$core..fmt..UpperHex$GT$3fmt17h3969f366df3d1826E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 1
    call $_ZN4core3fmt3num52_$LT$impl$u20$core..fmt..UpperHex$u20$for$u20$i8$GT$3fmt17hebc0280df4365f65E)
  (func $_ZN4core3fmt5Write10write_char17h9e8d63491b3d7364E (type 2) (param i32 i32) (result i32)
    (local i32 i32 i64 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    i32.const 0
    i32.store offset=4
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 1
            i32.const 128
            i32.lt_u
            br_if 0 (;@4;)
            local.get 1
            i32.const 2048
            i32.lt_u
            br_if 1 (;@3;)
            local.get 2
            i32.const 4
            i32.add
            local.set 3
            local.get 1
            i32.const 65536
            i32.ge_u
            br_if 2 (;@2;)
            local.get 2
            local.get 1
            i32.const 63
            i32.and
            i32.const 128
            i32.or
            i32.store8 offset=6
            local.get 2
            local.get 1
            i32.const 6
            i32.shr_u
            i32.const 63
            i32.and
            i32.const 128
            i32.or
            i32.store8 offset=5
            local.get 2
            local.get 1
            i32.const 12
            i32.shr_u
            i32.const 15
            i32.and
            i32.const 224
            i32.or
            i32.store8 offset=4
            i32.const 3
            local.set 1
            br 3 (;@1;)
          end
          local.get 2
          local.get 1
          i32.store8 offset=4
          local.get 2
          i32.const 4
          i32.add
          local.set 3
          i32.const 1
          local.set 1
          br 2 (;@1;)
        end
        local.get 2
        local.get 1
        i32.const 63
        i32.and
        i32.const 128
        i32.or
        i32.store8 offset=5
        local.get 2
        local.get 1
        i32.const 6
        i32.shr_u
        i32.const 31
        i32.and
        i32.const 192
        i32.or
        i32.store8 offset=4
        local.get 2
        i32.const 4
        i32.add
        local.set 3
        i32.const 2
        local.set 1
        br 1 (;@1;)
      end
      local.get 2
      local.get 1
      i32.const 63
      i32.and
      i32.const 128
      i32.or
      i32.store8 offset=7
      local.get 2
      local.get 1
      i32.const 18
      i32.shr_u
      i32.const 240
      i32.or
      i32.store8 offset=4
      local.get 2
      local.get 1
      i32.const 6
      i32.shr_u
      i32.const 63
      i32.and
      i32.const 128
      i32.or
      i32.store8 offset=6
      local.get 2
      local.get 1
      i32.const 12
      i32.shr_u
      i32.const 63
      i32.and
      i32.const 128
      i32.or
      i32.store8 offset=5
      i32.const 4
      local.set 1
    end
    local.get 2
    i32.const 8
    i32.add
    local.get 0
    i32.load
    local.get 3
    local.get 1
    call $_ZN3std2io5Write9write_all17ha85cbf0a744d542bE
    i32.const 0
    local.set 1
    block  ;; label = @1
      local.get 2
      i32.load8_u offset=8
      i32.const 3
      i32.eq
      br_if 0 (;@1;)
      local.get 2
      i64.load offset=8
      local.set 4
      block  ;; label = @2
        block  ;; label = @3
          i32.const 0
          br_if 0 (;@3;)
          local.get 0
          i32.load8_u offset=4
          i32.const 2
          i32.ne
          br_if 1 (;@2;)
        end
        local.get 0
        i32.const 8
        i32.add
        i32.load
        local.tee 1
        i32.load
        local.get 1
        i32.load offset=4
        i32.load
        call_indirect (type 0)
        block  ;; label = @3
          local.get 1
          i32.load offset=4
          local.tee 3
          i32.load offset=4
          local.tee 5
          i32.eqz
          br_if 0 (;@3;)
          local.get 1
          i32.load
          local.get 5
          local.get 3
          i32.load offset=8
          call $__rust_dealloc
        end
        local.get 0
        i32.load offset=8
        i32.const 12
        i32.const 4
        call $__rust_dealloc
      end
      local.get 0
      local.get 4
      i64.store offset=4 align=4
      i32.const 1
      local.set 1
    end
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN3std2io5Write9write_all17ha85cbf0a744d542bE (type 3) (param i32 i32 i32 i32)
    (local i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 4
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  local.get 3
                  i32.eqz
                  br_if 0 (;@7;)
                  loop  ;; label = @8
                    local.get 4
                    local.get 3
                    i32.store offset=12
                    local.get 4
                    local.get 2
                    i32.store offset=8
                    local.get 4
                    i32.const 16
                    i32.add
                    i32.const 2
                    local.get 4
                    i32.const 8
                    i32.add
                    i32.const 1
                    call $_ZN4wasi13lib_generated8fd_write17h599f0274fd7b3c57E
                    block  ;; label = @9
                      block  ;; label = @10
                        local.get 4
                        i32.load16_u offset=16
                        i32.const 1
                        i32.eq
                        br_if 0 (;@10;)
                        block  ;; label = @11
                          local.get 4
                          i32.load offset=20
                          local.tee 5
                          br_if 0 (;@11;)
                          i32.const 28
                          i32.const 1
                          call $__rust_alloc
                          local.tee 3
                          i32.eqz
                          br_if 8 (;@3;)
                          local.get 3
                          i32.const 24
                          i32.add
                          i32.const 0
                          i32.load offset=1051524 align=1
                          i32.store align=1
                          local.get 3
                          i32.const 16
                          i32.add
                          i32.const 0
                          i64.load offset=1051516 align=1
                          i64.store align=1
                          local.get 3
                          i32.const 8
                          i32.add
                          i32.const 0
                          i64.load offset=1051508 align=1
                          i64.store align=1
                          local.get 3
                          i32.const 0
                          i64.load offset=1051500 align=1
                          i64.store align=1
                          i32.const 12
                          i32.const 4
                          call $__rust_alloc
                          local.tee 2
                          i32.eqz
                          br_if 9 (;@2;)
                          local.get 2
                          i64.const 120259084316
                          i64.store offset=4 align=4
                          local.get 2
                          local.get 3
                          i32.store
                          i32.const 12
                          i32.const 4
                          call $__rust_alloc
                          local.tee 3
                          br_if 6 (;@5;)
                          i32.const 12
                          i32.const 4
                          call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
                          unreachable
                        end
                        local.get 3
                        local.get 5
                        i32.lt_u
                        br_if 9 (;@1;)
                        local.get 2
                        local.get 5
                        i32.add
                        local.set 2
                        local.get 3
                        local.get 5
                        i32.sub
                        local.set 3
                        br 1 (;@9;)
                      end
                      local.get 4
                      local.get 4
                      i32.load16_u offset=18
                      i32.store16 offset=30
                      local.get 4
                      i32.const 30
                      i32.add
                      call $_ZN4wasi5error5Error9raw_error17h3d77f281c47bf703E
                      i32.const 65535
                      i32.and
                      local.tee 5
                      call $_ZN3std3sys4wasi17decode_error_kind17h45e11c33b7590f34E
                      i32.const 255
                      i32.and
                      i32.const 15
                      i32.ne
                      br_if 3 (;@6;)
                    end
                    local.get 3
                    br_if 0 (;@8;)
                  end
                end
                local.get 0
                i32.const 3
                i32.store8
                br 2 (;@4;)
              end
              local.get 0
              i32.const 0
              i32.store
              local.get 0
              i32.const 4
              i32.add
              local.get 5
              i32.store
              br 1 (;@4;)
            end
            local.get 3
            i32.const 14
            i32.store8 offset=8
            local.get 3
            i32.const 1051084
            i32.store offset=4
            local.get 3
            local.get 2
            i32.store
            local.get 3
            local.get 4
            i32.load16_u offset=16 align=1
            i32.store16 offset=9 align=1
            local.get 3
            i32.const 11
            i32.add
            local.get 4
            i32.const 16
            i32.add
            i32.const 2
            i32.add
            i32.load8_u
            i32.store8
            local.get 0
            i32.const 4
            i32.add
            local.get 3
            i32.store
            local.get 0
            i32.const 2
            i32.store
          end
          local.get 4
          i32.const 32
          i32.add
          global.set 0
          return
        end
        i32.const 28
        i32.const 1
        call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
        unreachable
      end
      i32.const 12
      i32.const 4
      call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
      unreachable
    end
    local.get 5
    local.get 3
    call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
    unreachable)
  (func $_ZN4core3fmt5Write9write_fmt17hbd9d6bdcf58a73f8E (type 2) (param i32 i32) (result i32)
    (local i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 0
    i32.store offset=4
    local.get 2
    i32.const 8
    i32.add
    i32.const 16
    i32.add
    local.get 1
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    i32.const 8
    i32.add
    i32.const 8
    i32.add
    local.get 1
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    local.get 1
    i64.load align=4
    i64.store offset=8
    local.get 2
    i32.const 4
    i32.add
    i32.const 1050332
    local.get 2
    i32.const 8
    i32.add
    call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
    local.set 1
    local.get 2
    i32.const 32
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN3std9panicking12default_hook17hfcbeaa3f98c73639E (type 0) (param i32)
    (local i32 i32 i32 i32 i32 i64 i32)
    global.get 0
    i32.const 96
    i32.sub
    local.tee 1
    global.set 0
    i32.const 1
    local.set 2
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          i32.const 0
          i32.load offset=1059032
          i32.const 1
          i32.eq
          br_if 0 (;@3;)
          i32.const 0
          i64.const 1
          i64.store offset=1059032
          br 1 (;@2;)
        end
        i32.const 0
        i32.load offset=1059036
        i32.const 1
        i32.gt_u
        br_if 1 (;@1;)
      end
      block  ;; label = @2
        i32.const 0
        i32.load offset=1058984
        local.tee 2
        i32.const 2
        i32.le_u
        br_if 0 (;@2;)
        i32.const 1
        local.set 2
        br 1 (;@1;)
      end
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 2
            br_table 0 (;@4;) 1 (;@3;) 2 (;@2;) 0 (;@4;)
          end
          local.get 1
          i32.const 64
          i32.add
          i32.const 1050984
          i32.const 14
          call $_ZN3std3env7_var_os17h638254b5fcbbc440E
          block  ;; label = @4
            block  ;; label = @5
              local.get 1
              i32.load offset=64
              local.tee 3
              br_if 0 (;@5;)
              i32.const 5
              local.set 2
              br 1 (;@4;)
            end
            local.get 1
            i32.load offset=68
            local.set 4
            block  ;; label = @5
              block  ;; label = @6
                local.get 1
                i32.const 72
                i32.add
                i32.load
                i32.const -1
                i32.add
                local.tee 2
                i32.const 3
                i32.gt_u
                br_if 0 (;@6;)
                block  ;; label = @7
                  block  ;; label = @8
                    local.get 2
                    br_table 0 (;@8;) 2 (;@6;) 2 (;@6;) 1 (;@7;) 0 (;@8;)
                  end
                  i32.const 4
                  local.set 2
                  i32.const 1
                  local.set 5
                  local.get 3
                  i32.const 1050998
                  i32.eq
                  br_if 2 (;@5;)
                  local.get 3
                  i32.load8_u
                  i32.const 48
                  i32.ne
                  br_if 1 (;@6;)
                  br 2 (;@5;)
                end
                i32.const 1
                local.set 2
                i32.const 3
                local.set 5
                local.get 3
                i32.const 1051684
                i32.eq
                br_if 1 (;@5;)
                local.get 3
                i32.load align=1
                i32.const 1819047270
                i32.eq
                br_if 1 (;@5;)
              end
              i32.const 0
              local.set 2
              i32.const 2
              local.set 5
            end
            local.get 4
            i32.eqz
            br_if 0 (;@4;)
            local.get 3
            local.get 4
            i32.const 1
            call $__rust_dealloc
          end
          i32.const 0
          i32.const 1
          local.get 5
          local.get 2
          i32.const 5
          i32.eq
          local.tee 3
          select
          i32.store offset=1058984
          i32.const 4
          local.get 2
          local.get 3
          select
          local.set 2
          br 2 (;@1;)
        end
        i32.const 4
        local.set 2
        br 1 (;@1;)
      end
      i32.const 0
      local.set 2
    end
    local.get 1
    local.get 2
    i32.store8 offset=35
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 0
          call $_ZN4core5panic9PanicInfo8location17h30a49797d3e7ef56E
          local.tee 2
          i32.eqz
          br_if 0 (;@3;)
          local.get 1
          local.get 2
          i32.store offset=36
          local.get 1
          i32.const 24
          i32.add
          local.get 0
          call $_ZN4core5panic8Location4file17h54f5698a3003e7b3E
          local.get 1
          i32.load offset=24
          local.tee 2
          local.get 1
          i32.load offset=28
          i32.load offset=12
          call_indirect (type 1)
          local.set 6
          block  ;; label = @4
            local.get 2
            i32.eqz
            br_if 0 (;@4;)
            local.get 6
            i64.const 1229646359891580772
            i64.eq
            br_if 2 (;@2;)
          end
          local.get 1
          i32.const 16
          i32.add
          local.get 0
          call $_ZN4core5panic8Location4file17h54f5698a3003e7b3E
          local.get 1
          i32.load offset=16
          local.tee 2
          local.get 1
          i32.load offset=20
          i32.load offset=12
          call_indirect (type 1)
          local.set 6
          i32.const 8
          local.set 0
          i32.const 1051872
          local.set 5
          block  ;; label = @4
            local.get 2
            i32.eqz
            br_if 0 (;@4;)
            local.get 6
            i64.const 1326782401342502364
            i64.ne
            br_if 0 (;@4;)
            local.get 2
            i32.load offset=8
            local.set 0
            local.get 2
            i32.load
            local.set 5
          end
          local.get 1
          local.get 5
          i32.store offset=40
          br 2 (;@1;)
        end
        i32.const 1050555
        i32.const 43
        i32.const 1051856
        call $_ZN4core9panicking5panic17he9463ceb3e2615beE
        unreachable
      end
      local.get 1
      local.get 2
      i32.load
      i32.store offset=40
      local.get 2
      i32.load offset=4
      local.set 0
    end
    local.get 1
    local.get 0
    i32.store offset=44
    i32.const 0
    local.set 0
    block  ;; label = @1
      i32.const 0
      i32.load offset=1059020
      i32.const 1
      i32.eq
      br_if 0 (;@1;)
      i32.const 0
      i64.const 1
      i64.store offset=1059020 align=4
      i32.const 0
      i32.const 0
      i32.store offset=1059028
    end
    local.get 1
    i32.const 1059024
    call $_ZN3std10sys_common11thread_info10ThreadInfo4with28_$u7b$$u7b$closure$u7d$$u7d$17h7258ebfa61eae2eaE
    local.tee 2
    i32.store offset=52
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 2
          i32.load offset=16
          local.tee 5
          br_if 0 (;@3;)
          br 1 (;@2;)
        end
        local.get 2
        i32.const 16
        i32.add
        i32.const 0
        local.get 5
        select
        local.tee 0
        i32.load offset=4
        local.tee 3
        i32.const -1
        i32.add
        local.set 5
        local.get 3
        i32.eqz
        br_if 1 (;@1;)
        local.get 0
        i32.load
        local.set 0
      end
      local.get 1
      local.get 5
      i32.const 9
      local.get 0
      select
      i32.store offset=60
      local.get 1
      local.get 0
      i32.const 1051880
      local.get 0
      select
      i32.store offset=56
      local.get 1
      local.get 1
      i32.const 35
      i32.add
      i32.store offset=76
      local.get 1
      local.get 1
      i32.const 36
      i32.add
      i32.store offset=72
      local.get 1
      local.get 1
      i32.const 40
      i32.add
      i32.store offset=68
      local.get 1
      local.get 1
      i32.const 56
      i32.add
      i32.store offset=64
      i32.const 0
      local.set 3
      local.get 1
      i32.const 8
      i32.add
      i32.const 0
      local.get 1
      call $_ZN3std2io5stdio9set_panic17h18c62f96637563b7E
      local.get 1
      i32.load offset=12
      local.set 5
      block  ;; label = @2
        block  ;; label = @3
          local.get 1
          i32.load offset=8
          local.tee 0
          i32.eqz
          br_if 0 (;@3;)
          local.get 1
          local.get 5
          i32.store offset=84
          local.get 1
          local.get 0
          i32.store offset=80
          local.get 1
          i32.const 64
          i32.add
          local.get 1
          i32.const 80
          i32.add
          i32.const 1051928
          call $_ZN3std9panicking12default_hook28_$u7b$$u7b$closure$u7d$$u7d$17hd5e959ee7cc2957bE
          local.get 1
          local.get 1
          i32.load offset=80
          local.get 1
          i32.load offset=84
          call $_ZN3std2io5stdio9set_panic17h18c62f96637563b7E
          block  ;; label = @4
            local.get 1
            i32.load
            local.tee 3
            i32.eqz
            br_if 0 (;@4;)
            local.get 3
            local.get 1
            i32.load offset=4
            local.tee 4
            i32.load
            call_indirect (type 0)
            local.get 4
            i32.load offset=4
            local.tee 7
            i32.eqz
            br_if 0 (;@4;)
            local.get 3
            local.get 7
            local.get 4
            i32.load offset=8
            call $__rust_dealloc
          end
          i32.const 1
          local.set 3
          br 1 (;@2;)
        end
        local.get 1
        i32.const 64
        i32.add
        local.get 1
        i32.const 88
        i32.add
        i32.const 1051892
        call $_ZN3std9panicking12default_hook28_$u7b$$u7b$closure$u7d$$u7d$17hd5e959ee7cc2957bE
      end
      local.get 2
      local.get 2
      i32.load
      local.tee 4
      i32.const -1
      i32.add
      i32.store
      block  ;; label = @2
        local.get 4
        i32.const 1
        i32.ne
        br_if 0 (;@2;)
        local.get 1
        i32.const 52
        i32.add
        call $_ZN5alloc4sync12Arc$LT$T$GT$9drop_slow17h35b378a36cfe00ecE
      end
      block  ;; label = @2
        local.get 0
        i32.const 0
        i32.ne
        local.get 3
        i32.const 1
        i32.xor
        i32.and
        i32.eqz
        br_if 0 (;@2;)
        local.get 0
        local.get 5
        i32.load
        call_indirect (type 0)
        local.get 5
        i32.load offset=4
        local.tee 2
        i32.eqz
        br_if 0 (;@2;)
        local.get 0
        local.get 2
        local.get 5
        i32.load offset=8
        call $__rust_dealloc
      end
      local.get 1
      i32.const 96
      i32.add
      global.set 0
      return
    end
    local.get 5
    i32.const 0
    call $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E
    unreachable)
  (func $_ZN3std9panicking11begin_panic17h4f03e37f2089bbdaE (type 5) (param i32 i32 i32)
    (local i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    local.get 1
    i32.store offset=12
    local.get 3
    local.get 0
    i32.store offset=8
    local.get 3
    i32.const 8
    i32.add
    i32.const 1052180
    i32.const 0
    local.get 2
    call $_ZN4core5panic8Location6caller17hba7ec45f0d210bdeE
    call $_ZN3std9panicking20rust_panic_with_hook17h8bf13b9f643a54b1E
    unreachable)
  (func $_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h56b65778f93a50acE (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 3
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.load
        i32.const 1
        i32.ne
        br_if 0 (;@2;)
        i32.const 1051688
        local.set 2
        i32.const 9
        local.set 4
        br 1 (;@1;)
      end
      local.get 3
      i32.const 16
      i32.add
      local.get 2
      i32.load offset=4
      local.get 2
      i32.const 8
      i32.add
      i32.load
      call $_ZN4core3str9from_utf817h40c83401242cc090E
      i32.const 1051688
      local.get 3
      i32.load offset=20
      local.get 3
      i32.load offset=16
      i32.const 1
      i32.eq
      local.tee 4
      select
      local.set 2
      i32.const 9
      local.get 3
      i32.const 16
      i32.add
      i32.const 8
      i32.add
      i32.load
      local.get 4
      select
      local.set 4
    end
    local.get 3
    i32.const 8
    i32.add
    local.get 2
    local.get 4
    call $_ZN4core3str5lossy9Utf8Lossy10from_bytes17h1357f46792efee29E
    local.get 3
    i32.load offset=8
    local.get 3
    i32.load offset=12
    local.get 1
    call $_ZN66_$LT$core..str..lossy..Utf8Lossy$u20$as$u20$core..fmt..Display$GT$3fmt17haeaf8cd40e5faeccE
    local.set 2
    block  ;; label = @1
      local.get 0
      i32.load
      local.tee 1
      i32.eqz
      br_if 0 (;@1;)
      local.get 0
      i32.load offset=4
      local.tee 0
      i32.eqz
      br_if 0 (;@1;)
      local.get 1
      local.get 0
      i32.const 1
      call $__rust_dealloc
    end
    local.get 3
    i32.const 32
    i32.add
    global.set 0
    local.get 2)
  (func $_ZN4core3ptr13drop_in_place17h025d405e835bf726E (type 0) (param i32))
  (func $_ZN4core3ptr13drop_in_place17h08a8a1ed893b2be6E (type 0) (param i32)
    (local i32 i32)
    local.get 0
    i32.load
    local.get 0
    i32.load offset=4
    i32.load
    call_indirect (type 0)
    block  ;; label = @1
      local.get 0
      i32.load offset=4
      local.tee 1
      i32.load offset=4
      local.tee 2
      i32.eqz
      br_if 0 (;@1;)
      local.get 0
      i32.load
      local.get 2
      local.get 1
      i32.load offset=8
      call $__rust_dealloc
    end)
  (func $_ZN4core3ptr13drop_in_place17h242a50c8a365baa2E (type 0) (param i32)
    (local i32)
    block  ;; label = @1
      local.get 0
      i32.const 8
      i32.add
      i32.load
      local.tee 1
      i32.eqz
      br_if 0 (;@1;)
      local.get 0
      i32.load offset=4
      local.get 1
      i32.const 1
      call $__rust_dealloc
    end)
  (func $_ZN4core3ptr13drop_in_place17h2a6c181863033dd6E (type 0) (param i32)
    (local i32)
    block  ;; label = @1
      local.get 0
      i32.load offset=4
      local.tee 1
      i32.eqz
      br_if 0 (;@1;)
      local.get 0
      i32.load
      local.get 1
      i32.const 1
      call $__rust_dealloc
    end)
  (func $_ZN4core3ptr13drop_in_place17hcd0ca0b4ad624647E (type 0) (param i32)
    (local i32 i32 i32)
    block  ;; label = @1
      block  ;; label = @2
        i32.const 0
        br_if 0 (;@2;)
        local.get 0
        i32.load8_u offset=4
        i32.const 2
        i32.ne
        br_if 1 (;@1;)
      end
      local.get 0
      i32.const 8
      i32.add
      i32.load
      local.tee 1
      i32.load
      local.get 1
      i32.load offset=4
      i32.load
      call_indirect (type 0)
      block  ;; label = @2
        local.get 1
        i32.load offset=4
        local.tee 2
        i32.load offset=4
        local.tee 3
        i32.eqz
        br_if 0 (;@2;)
        local.get 1
        i32.load
        local.get 3
        local.get 2
        i32.load offset=8
        call $__rust_dealloc
      end
      local.get 0
      i32.load offset=8
      i32.const 12
      i32.const 4
      call $__rust_dealloc
    end)
  (func $_ZN4core3ptr13drop_in_place17he00a11dd07d03937E (type 0) (param i32)
    (local i32)
    block  ;; label = @1
      local.get 0
      i32.load offset=4
      local.tee 1
      i32.eqz
      br_if 0 (;@1;)
      local.get 0
      i32.const 8
      i32.add
      i32.load
      local.tee 0
      i32.eqz
      br_if 0 (;@1;)
      local.get 1
      local.get 0
      i32.const 1
      call $__rust_dealloc
    end)
  (func $_ZN4core3ptr13drop_in_place17hf93883f47f141821E (type 0) (param i32)
    (local i32)
    block  ;; label = @1
      local.get 0
      i32.load
      local.tee 1
      i32.eqz
      br_if 0 (;@1;)
      local.get 0
      i32.load offset=4
      local.tee 0
      i32.eqz
      br_if 0 (;@1;)
      local.get 1
      local.get 0
      i32.const 1
      call $__rust_dealloc
    end)
  (func $_ZN4core6option15Option$LT$T$GT$6unwrap17h170a589b0290d63aE (type 2) (param i32 i32) (result i32)
    block  ;; label = @1
      local.get 0
      br_if 0 (;@1;)
      i32.const 1050555
      i32.const 43
      local.get 1
      call $_ZN4core9panicking5panic17he9463ceb3e2615beE
      unreachable
    end
    local.get 0)
  (func $_ZN4core6option15Option$LT$T$GT$6unwrap17h1739ecdc6eda48e2E (type 14) (param i32) (result i32)
    block  ;; label = @1
      local.get 0
      br_if 0 (;@1;)
      i32.const 1050555
      i32.const 43
      i32.const 1052128
      call $_ZN4core9panicking5panic17he9463ceb3e2615beE
      unreachable
    end
    local.get 0)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$10write_char17h3aa90f5249f239f0E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 1
    call $_ZN4core3fmt5Write10write_char17h9e8d63491b3d7364E)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$10write_char17h8a06baf7e7cbda40E (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    local.get 0
    i32.load
    local.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 1
            i32.const 128
            i32.lt_u
            br_if 0 (;@4;)
            local.get 2
            i32.const 0
            i32.store offset=12
            local.get 1
            i32.const 2048
            i32.lt_u
            br_if 1 (;@3;)
            local.get 2
            i32.const 12
            i32.add
            local.set 3
            block  ;; label = @5
              local.get 1
              i32.const 65536
              i32.ge_u
              br_if 0 (;@5;)
              local.get 2
              local.get 1
              i32.const 63
              i32.and
              i32.const 128
              i32.or
              i32.store8 offset=14
              local.get 2
              local.get 1
              i32.const 6
              i32.shr_u
              i32.const 63
              i32.and
              i32.const 128
              i32.or
              i32.store8 offset=13
              local.get 2
              local.get 1
              i32.const 12
              i32.shr_u
              i32.const 15
              i32.and
              i32.const 224
              i32.or
              i32.store8 offset=12
              i32.const 3
              local.set 1
              br 3 (;@2;)
            end
            local.get 2
            local.get 1
            i32.const 63
            i32.and
            i32.const 128
            i32.or
            i32.store8 offset=15
            local.get 2
            local.get 1
            i32.const 18
            i32.shr_u
            i32.const 240
            i32.or
            i32.store8 offset=12
            local.get 2
            local.get 1
            i32.const 6
            i32.shr_u
            i32.const 63
            i32.and
            i32.const 128
            i32.or
            i32.store8 offset=14
            local.get 2
            local.get 1
            i32.const 12
            i32.shr_u
            i32.const 63
            i32.and
            i32.const 128
            i32.or
            i32.store8 offset=13
            i32.const 4
            local.set 1
            br 2 (;@2;)
          end
          block  ;; label = @4
            local.get 0
            i32.load offset=8
            local.tee 3
            local.get 0
            i32.load offset=4
            i32.ne
            br_if 0 (;@4;)
            local.get 0
            i32.const 1
            call $_ZN5alloc3vec12Vec$LT$T$GT$7reserve17h7b6410b58a933b43E
            local.get 0
            i32.load offset=8
            local.set 3
          end
          local.get 0
          i32.load
          local.get 3
          i32.add
          local.get 1
          i32.store8
          local.get 0
          local.get 0
          i32.load offset=8
          i32.const 1
          i32.add
          i32.store offset=8
          br 2 (;@1;)
        end
        local.get 2
        local.get 1
        i32.const 63
        i32.and
        i32.const 128
        i32.or
        i32.store8 offset=13
        local.get 2
        local.get 1
        i32.const 6
        i32.shr_u
        i32.const 31
        i32.and
        i32.const 192
        i32.or
        i32.store8 offset=12
        local.get 2
        i32.const 12
        i32.add
        local.set 3
        i32.const 2
        local.set 1
      end
      local.get 0
      local.get 1
      call $_ZN5alloc3vec12Vec$LT$T$GT$7reserve17h7b6410b58a933b43E
      local.get 0
      local.get 0
      i32.load offset=8
      local.tee 4
      local.get 1
      i32.add
      i32.store offset=8
      local.get 4
      local.get 0
      i32.load
      i32.add
      local.get 3
      local.get 1
      call $memcpy
      drop
    end
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    i32.const 0)
  (func $_ZN5alloc3vec12Vec$LT$T$GT$7reserve17h7b6410b58a933b43E (type 4) (param i32 i32)
    (local i32 i32)
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 0
          i32.load offset=4
          local.tee 2
          local.get 0
          i32.load offset=8
          local.tee 3
          i32.sub
          local.get 1
          i32.ge_u
          br_if 0 (;@3;)
          local.get 3
          local.get 1
          i32.add
          local.tee 1
          local.get 3
          i32.lt_u
          br_if 2 (;@1;)
          local.get 2
          i32.const 1
          i32.shl
          local.tee 3
          local.get 1
          local.get 3
          local.get 1
          i32.gt_u
          select
          local.tee 1
          i32.const 0
          i32.lt_s
          br_if 2 (;@1;)
          block  ;; label = @4
            block  ;; label = @5
              local.get 2
              br_if 0 (;@5;)
              local.get 1
              i32.const 1
              call $__rust_alloc
              local.set 2
              br 1 (;@4;)
            end
            local.get 0
            i32.load
            local.get 2
            i32.const 1
            local.get 1
            call $__rust_realloc
            local.set 2
          end
          local.get 2
          i32.eqz
          br_if 1 (;@2;)
          local.get 0
          local.get 1
          i32.store offset=4
          local.get 0
          local.get 2
          i32.store
        end
        return
      end
      local.get 1
      i32.const 1
      call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
      unreachable
    end
    call $_ZN5alloc7raw_vec17capacity_overflow17h60fd539dfca5134dE
    unreachable)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_fmt17h62784e4259f969c4E (type 2) (param i32 i32) (result i32)
    (local i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 0
    i32.load
    i32.store offset=4
    local.get 2
    i32.const 8
    i32.add
    i32.const 16
    i32.add
    local.get 1
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    i32.const 8
    i32.add
    i32.const 8
    i32.add
    local.get 1
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    local.get 1
    i64.load align=4
    i64.store offset=8
    local.get 2
    i32.const 4
    i32.add
    i32.const 1050356
    local.get 2
    i32.const 8
    i32.add
    call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
    local.set 1
    local.get 2
    i32.const 32
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_fmt17h9f3c0397574cf561E (type 2) (param i32 i32) (result i32)
    (local i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 0
    i32.load
    i32.store offset=4
    local.get 2
    i32.const 8
    i32.add
    i32.const 16
    i32.add
    local.get 1
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    i32.const 8
    i32.add
    i32.const 8
    i32.add
    local.get 1
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    local.get 1
    i64.load align=4
    i64.store offset=8
    local.get 2
    i32.const 4
    i32.add
    i32.const 1050332
    local.get 2
    i32.const 8
    i32.add
    call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
    local.set 1
    local.get 2
    i32.const 32
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_str17h1bea73a017e579a3E (type 6) (param i32 i32 i32) (result i32)
    (local i32)
    local.get 0
    i32.load
    local.tee 0
    local.get 2
    call $_ZN5alloc3vec12Vec$LT$T$GT$7reserve17h7b6410b58a933b43E
    local.get 0
    local.get 0
    i32.load offset=8
    local.tee 3
    local.get 2
    i32.add
    i32.store offset=8
    local.get 3
    local.get 0
    i32.load
    i32.add
    local.get 1
    local.get 2
    call $memcpy
    drop
    i32.const 0)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_str17hcb1cbfc4948dd346E (type 6) (param i32 i32 i32) (result i32)
    (local i32 i64 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    i32.const 8
    i32.add
    local.get 0
    i32.load
    local.tee 0
    i32.load
    local.get 1
    local.get 2
    call $_ZN3std2io5Write9write_all17ha85cbf0a744d542bE
    i32.const 0
    local.set 1
    block  ;; label = @1
      local.get 3
      i32.load8_u offset=8
      i32.const 3
      i32.eq
      br_if 0 (;@1;)
      local.get 3
      i64.load offset=8
      local.set 4
      block  ;; label = @2
        block  ;; label = @3
          i32.const 0
          br_if 0 (;@3;)
          local.get 0
          i32.load8_u offset=4
          i32.const 2
          i32.ne
          br_if 1 (;@2;)
        end
        local.get 0
        i32.const 8
        i32.add
        i32.load
        local.tee 1
        i32.load
        local.get 1
        i32.load offset=4
        i32.load
        call_indirect (type 0)
        block  ;; label = @3
          local.get 1
          i32.load offset=4
          local.tee 2
          i32.load offset=4
          local.tee 5
          i32.eqz
          br_if 0 (;@3;)
          local.get 1
          i32.load
          local.get 5
          local.get 2
          i32.load offset=8
          call $__rust_dealloc
        end
        local.get 0
        i32.load offset=8
        i32.const 12
        i32.const 4
        call $__rust_dealloc
      end
      local.get 0
      local.get 4
      i64.store offset=4 align=4
      i32.const 1
      local.set 1
    end
    local.get 3
    i32.const 16
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN5alloc4sync12Arc$LT$T$GT$9drop_slow17h35b378a36cfe00ecE (type 0) (param i32)
    (local i32 i32)
    block  ;; label = @1
      local.get 0
      i32.load
      local.tee 1
      i32.const 16
      i32.add
      i32.load
      local.tee 2
      i32.eqz
      br_if 0 (;@1;)
      local.get 2
      i32.const 0
      i32.store8
      local.get 1
      i32.const 20
      i32.add
      i32.load
      local.tee 2
      i32.eqz
      br_if 0 (;@1;)
      local.get 1
      i32.load offset=16
      local.get 2
      i32.const 1
      call $__rust_dealloc
    end
    local.get 1
    i32.const 28
    i32.add
    i32.load
    i32.const 1
    i32.const 1
    call $__rust_dealloc
    local.get 0
    i32.load
    local.tee 1
    local.get 1
    i32.load offset=4
    local.tee 1
    i32.const -1
    i32.add
    i32.store offset=4
    block  ;; label = @1
      local.get 1
      i32.const 1
      i32.ne
      br_if 0 (;@1;)
      local.get 0
      i32.load
      i32.const 48
      i32.const 8
      call $__rust_dealloc
    end)
  (func $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$11allocate_in28_$u7b$$u7b$closure$u7d$$u7d$17h0fefaba762b2b53dE (type 9)
    call $_ZN5alloc7raw_vec17capacity_overflow17h60fd539dfca5134dE
    unreachable)
  (func $_ZN60_$LT$alloc..string..String$u20$as$u20$core..fmt..Display$GT$3fmt17hf4bdb13e62be459bE (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 0
    i32.load offset=8
    local.get 1
    call $_ZN42_$LT$str$u20$as$u20$core..fmt..Display$GT$3fmt17hcf977a4d08f25cc0E)
  (func $_ZN3std10sys_common11thread_info10ThreadInfo4with28_$u7b$$u7b$closure$u7d$$u7d$17h7258ebfa61eae2eaE (type 14) (param i32) (result i32)
    (local i32 i32 i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 1
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 0
            i32.load
            local.tee 2
            i32.const 1
            i32.add
            i32.const 0
            i32.le_s
            br_if 0 (;@4;)
            local.get 0
            local.get 2
            i32.store
            block  ;; label = @5
              local.get 0
              i32.load offset=4
              local.tee 3
              br_if 0 (;@5;)
              local.get 1
              i32.const 0
              i32.store offset=8
              local.get 1
              i32.const 8
              i32.add
              call $_ZN3std6thread6Thread3new17h913808035e83dacfE
              local.set 3
              local.get 0
              i32.load
              br_if 2 (;@3;)
              local.get 0
              i32.const -1
              i32.store
              block  ;; label = @6
                local.get 0
                i32.load offset=4
                local.tee 2
                i32.eqz
                br_if 0 (;@6;)
                local.get 2
                local.get 2
                i32.load
                local.tee 4
                i32.const -1
                i32.add
                i32.store
                local.get 4
                i32.const 1
                i32.ne
                br_if 0 (;@6;)
                local.get 0
                i32.const 4
                i32.add
                call $_ZN5alloc4sync12Arc$LT$T$GT$9drop_slow17h35b378a36cfe00ecE
              end
              local.get 0
              local.get 3
              i32.store offset=4
              local.get 0
              local.get 0
              i32.load
              i32.const 1
              i32.add
              local.tee 2
              i32.store
            end
            local.get 2
            br_if 2 (;@2;)
            local.get 0
            i32.const -1
            i32.store
            local.get 3
            local.get 3
            i32.load
            local.tee 2
            i32.const 1
            i32.add
            i32.store
            local.get 2
            i32.const -1
            i32.le_s
            br_if 3 (;@1;)
            local.get 0
            local.get 0
            i32.load
            i32.const 1
            i32.add
            i32.store
            local.get 1
            i32.const 32
            i32.add
            global.set 0
            local.get 3
            return
          end
          i32.const 1050496
          i32.const 24
          local.get 1
          i32.const 24
          i32.add
          i32.const 1050632
          i32.const 1050520
          call $_ZN4core6option18expect_none_failed17h5718e8afd751d0acE
          unreachable
        end
        i32.const 1050396
        i32.const 16
        local.get 1
        i32.const 24
        i32.add
        i32.const 1050600
        i32.const 1050480
        call $_ZN4core6option18expect_none_failed17h5718e8afd751d0acE
        unreachable
      end
      i32.const 1050396
      i32.const 16
      local.get 1
      i32.const 24
      i32.add
      i32.const 1050600
      i32.const 1050480
      call $_ZN4core6option18expect_none_failed17h5718e8afd751d0acE
      unreachable
    end
    unreachable
    unreachable)
  (func $_ZN3std9panicking15begin_panic_fmt17hdff55c3855d10ed6E (type 4) (param i32 i32)
    (local i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 1
    call $_ZN4core5panic8Location6caller17hba7ec45f0d210bdeE
    i32.store offset=12
    local.get 2
    local.get 0
    i32.store offset=8
    local.get 2
    i32.const 1050536
    i32.store offset=4
    local.get 2
    i32.const 1050536
    i32.store
    local.get 2
    call $rust_begin_unwind
    unreachable)
  (func $_ZN3std6thread6Thread3new17h913808035e83dacfE (type 14) (param i32) (result i32)
    (local i32 i32 i32 i32 i64)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 1
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                local.get 0
                i32.load
                local.tee 2
                br_if 0 (;@6;)
                i32.const 0
                local.set 3
                br 1 (;@5;)
              end
              local.get 1
              local.get 0
              i64.load offset=4 align=4
              i64.store offset=36 align=4
              local.get 1
              local.get 2
              i32.store offset=32
              local.get 1
              i32.const 16
              i32.add
              local.get 1
              i32.const 32
              i32.add
              call $_ZN5alloc6string104_$LT$impl$u20$core..convert..From$LT$alloc..string..String$GT$$u20$for$u20$alloc..vec..Vec$LT$u8$GT$$GT$4from17hdf1aaa94a1e2e337E
              local.get 1
              i32.const 8
              i32.add
              i32.const 0
              local.get 1
              i32.load offset=16
              local.tee 0
              local.get 1
              i32.load offset=24
              call $_ZN4core5slice6memchr6memchr17h5dbdc97a74440bacE
              local.get 1
              i32.load offset=8
              br_if 1 (;@4;)
              local.get 1
              i32.const 32
              i32.add
              i32.const 8
              i32.add
              local.get 1
              i32.const 16
              i32.add
              i32.const 8
              i32.add
              i32.load
              i32.store
              local.get 1
              local.get 1
              i64.load offset=16
              i64.store offset=32
              local.get 1
              local.get 1
              i32.const 32
              i32.add
              call $_ZN3std3ffi5c_str7CString18from_vec_unchecked17hc4f44fb3d61eff8fE
              local.get 1
              i32.load offset=4
              local.set 4
              local.get 1
              i32.load
              local.set 3
            end
            i32.const 0
            i32.load8_u offset=1059040
            br_if 1 (;@3;)
            i32.const 0
            i32.const 1
            i32.store8 offset=1059040
            block  ;; label = @5
              block  ;; label = @6
                i32.const 0
                i64.load offset=1058968
                local.tee 5
                i64.const -1
                i64.eq
                br_if 0 (;@6;)
                i32.const 0
                local.get 5
                i64.const 1
                i64.add
                i64.store offset=1058968
                local.get 5
                i64.const 0
                i64.ne
                br_if 1 (;@5;)
                i32.const 1050555
                i32.const 43
                i32.const 1050904
                call $_ZN4core9panicking5panic17he9463ceb3e2615beE
                unreachable
              end
              i32.const 1050832
              i32.const 55
              i32.const 1050888
              call $_ZN3std9panicking11begin_panic17h4f03e37f2089bbdaE
              unreachable
            end
            i32.const 0
            i32.const 0
            i32.store8 offset=1059040
            i32.const 1
            i32.const 1
            call $__rust_alloc
            local.tee 2
            i32.eqz
            br_if 2 (;@2;)
            local.get 2
            i32.const 0
            i32.store8
            i32.const 48
            i32.const 8
            call $__rust_alloc
            local.tee 0
            i32.eqz
            br_if 3 (;@1;)
            local.get 0
            i64.const 1
            i64.store offset=36 align=4
            local.get 0
            i32.const 0
            i32.store offset=24
            local.get 0
            local.get 4
            i32.store offset=20
            local.get 0
            local.get 3
            i32.store offset=16
            local.get 0
            local.get 5
            i64.store offset=8
            local.get 0
            i64.const 4294967297
            i64.store
            local.get 0
            local.get 2
            i64.extend_i32_u
            i64.store offset=28 align=4
            local.get 1
            i32.const 48
            i32.add
            global.set 0
            local.get 0
            return
          end
          local.get 1
          i32.load offset=12
          local.set 2
          local.get 1
          i32.const 40
          i32.add
          local.get 1
          i64.load offset=20 align=4
          i64.store
          local.get 1
          local.get 0
          i32.store offset=36
          local.get 1
          local.get 2
          i32.store offset=32
          i32.const 1050920
          i32.const 47
          local.get 1
          i32.const 32
          i32.add
          i32.const 1050616
          i32.const 1050968
          call $_ZN4core6option18expect_none_failed17h5718e8afd751d0acE
          unreachable
        end
        i32.const 1052392
        i32.const 32
        i32.const 1052460
        call $_ZN3std9panicking11begin_panic17h4f03e37f2089bbdaE
        unreachable
      end
      i32.const 1
      i32.const 1
      call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
      unreachable
    end
    i32.const 48
    i32.const 8
    call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
    unreachable)
  (func $_ZN3std3ffi5c_str7CString18from_vec_unchecked17hc4f44fb3d61eff8fE (type 4) (param i32 i32)
    (local i32 i32 i32 i32)
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              local.get 1
              i32.load offset=4
              local.tee 2
              local.get 1
              i32.load offset=8
              local.tee 3
              i32.ne
              br_if 0 (;@5;)
              local.get 3
              i32.const 1
              i32.add
              local.tee 2
              local.get 3
              i32.lt_u
              br_if 2 (;@3;)
              local.get 2
              i32.const 0
              i32.lt_s
              br_if 2 (;@3;)
              block  ;; label = @6
                block  ;; label = @7
                  local.get 3
                  br_if 0 (;@7;)
                  local.get 2
                  i32.const 1
                  call $__rust_alloc
                  local.set 4
                  br 1 (;@6;)
                end
                local.get 1
                i32.load
                local.get 3
                i32.const 1
                local.get 2
                call $__rust_realloc
                local.set 4
              end
              local.get 4
              i32.eqz
              br_if 1 (;@4;)
              local.get 1
              local.get 2
              i32.store offset=4
              local.get 1
              local.get 4
              i32.store
            end
            block  ;; label = @5
              local.get 3
              local.get 2
              i32.ne
              br_if 0 (;@5;)
              local.get 1
              i32.const 1
              call $_ZN5alloc3vec12Vec$LT$T$GT$7reserve17h7b6410b58a933b43E
              local.get 1
              i32.load offset=4
              local.set 2
              local.get 1
              i32.load offset=8
              local.set 3
            end
            local.get 1
            local.get 3
            i32.const 1
            i32.add
            local.tee 4
            i32.store offset=8
            local.get 1
            i32.load
            local.tee 5
            local.get 3
            i32.add
            i32.const 0
            i32.store8
            block  ;; label = @5
              local.get 2
              local.get 4
              i32.ne
              br_if 0 (;@5;)
              local.get 5
              local.set 1
              local.get 2
              local.set 4
              br 4 (;@1;)
            end
            local.get 2
            local.get 4
            i32.lt_u
            br_if 2 (;@2;)
            block  ;; label = @5
              local.get 4
              br_if 0 (;@5;)
              i32.const 0
              local.set 4
              i32.const 1
              local.set 1
              local.get 2
              i32.eqz
              br_if 4 (;@1;)
              local.get 5
              local.get 2
              i32.const 1
              call $__rust_dealloc
              br 4 (;@1;)
            end
            local.get 5
            local.get 2
            i32.const 1
            local.get 4
            call $__rust_realloc
            local.tee 1
            br_if 3 (;@1;)
            local.get 4
            i32.const 1
            call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
            unreachable
          end
          local.get 2
          i32.const 1
          call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
          unreachable
        end
        call $_ZN5alloc7raw_vec17capacity_overflow17h60fd539dfca5134dE
        unreachable
      end
      i32.const 1050756
      i32.const 36
      i32.const 1050740
      call $_ZN4core9panicking5panic17he9463ceb3e2615beE
      unreachable
    end
    local.get 0
    local.get 4
    i32.store offset=4
    local.get 0
    local.get 1
    i32.store)
  (func $_ZN3std3env7_var_os17h638254b5fcbbc440E (type 5) (param i32 i32 i32)
    (local i32 i32 i32 i32 i32 i32 i64)
    global.get 0
    i32.const 80
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    local.get 2
    i32.store offset=28
    local.get 3
    local.get 1
    i32.store offset=24
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 2
          i32.const 1
          i32.add
          local.tee 4
          i32.const -1
          i32.le_s
          br_if 0 (;@3;)
          block  ;; label = @4
            block  ;; label = @5
              local.get 4
              i32.eqz
              br_if 0 (;@5;)
              local.get 4
              i32.const 1
              call $__rust_alloc
              local.tee 5
              br_if 1 (;@4;)
              local.get 4
              i32.const 1
              call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
              unreachable
            end
            call $_ZN5alloc7raw_vec17capacity_overflow17h60fd539dfca5134dE
            unreachable
          end
          i32.const 0
          local.set 6
          local.get 3
          i32.const 16
          i32.add
          i32.const 0
          local.get 5
          local.get 1
          local.get 2
          call $memcpy
          local.tee 1
          local.get 2
          call $_ZN4core5slice6memchr6memchr17h5dbdc97a74440bacE
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                local.get 3
                i32.load offset=16
                br_if 0 (;@6;)
                local.get 3
                local.get 2
                i32.store offset=48
                local.get 3
                local.get 4
                i32.store offset=44
                local.get 3
                local.get 1
                i32.store offset=40
                local.get 3
                i32.const 8
                i32.add
                local.get 3
                i32.const 40
                i32.add
                call $_ZN3std3ffi5c_str7CString18from_vec_unchecked17hc4f44fb3d61eff8fE
                local.get 3
                i32.load offset=12
                local.set 7
                local.get 3
                i32.load offset=8
                local.tee 8
                call $getenv
                local.tee 5
                br_if 1 (;@5;)
                br 2 (;@4;)
              end
              local.get 3
              i32.load offset=20
              local.set 6
              local.get 3
              i32.const 40
              i32.add
              i32.const 12
              i32.add
              local.get 2
              i32.store
              local.get 3
              i32.const 48
              i32.add
              local.get 4
              i32.store
              local.get 3
              local.get 1
              i32.store offset=44
              local.get 3
              local.get 6
              i32.store offset=40
              local.get 3
              i32.const 64
              i32.add
              local.get 3
              i32.const 40
              i32.add
              call $_ZN3std3ffi5c_str104_$LT$impl$u20$core..convert..From$LT$std..ffi..c_str..NulError$GT$$u20$for$u20$std..io..error..Error$GT$4from17hf5367864c69978b6E
              local.get 3
              local.get 3
              i64.load offset=64
              i64.store offset=32
              local.get 3
              i32.const 60
              i32.add
              i32.const 2
              i32.store
              local.get 3
              i32.const 64
              i32.add
              i32.const 12
              i32.add
              i32.const 23
              i32.store
              local.get 3
              i64.const 2
              i64.store offset=44 align=4
              local.get 3
              i32.const 1051052
              i32.store offset=40
              local.get 3
              i32.const 24
              i32.store offset=68
              local.get 3
              local.get 3
              i32.const 64
              i32.add
              i32.store offset=56
              local.get 3
              local.get 3
              i32.const 32
              i32.add
              i32.store offset=72
              local.get 3
              local.get 3
              i32.const 24
              i32.add
              i32.store offset=64
              local.get 3
              i32.const 40
              i32.add
              i32.const 1051068
              call $_ZN3std9panicking15begin_panic_fmt17hdff55c3855d10ed6E
              unreachable
            end
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  local.get 5
                  i32.load8_u
                  i32.eqz
                  br_if 0 (;@7;)
                  local.get 5
                  i32.const 1
                  i32.add
                  local.set 6
                  i32.const 0
                  local.set 2
                  loop  ;; label = @8
                    local.get 6
                    local.get 2
                    i32.add
                    local.set 4
                    local.get 2
                    i32.const 1
                    i32.add
                    local.tee 1
                    local.set 2
                    local.get 4
                    i32.load8_u
                    br_if 0 (;@8;)
                  end
                  local.get 1
                  i32.const -1
                  i32.eq
                  br_if 5 (;@2;)
                  local.get 1
                  i32.const -1
                  i32.le_s
                  br_if 4 (;@3;)
                  local.get 1
                  br_if 1 (;@6;)
                end
                i32.const 1
                local.set 6
                i32.const 0
                local.set 1
                br 1 (;@5;)
              end
              local.get 1
              i32.const 1
              call $__rust_alloc
              local.tee 6
              i32.eqz
              br_if 4 (;@1;)
            end
            local.get 6
            local.get 5
            local.get 1
            call $memcpy
            drop
            local.get 1
            i64.extend_i32_u
            local.tee 9
            i64.const 32
            i64.shl
            local.get 9
            i64.or
            local.set 9
          end
          local.get 8
          i32.const 0
          i32.store8
          local.get 9
          i64.const 32
          i64.shr_u
          i32.wrap_i64
          local.set 2
          local.get 9
          i32.wrap_i64
          local.set 4
          block  ;; label = @4
            local.get 7
            i32.eqz
            br_if 0 (;@4;)
            local.get 8
            local.get 7
            i32.const 1
            call $__rust_dealloc
          end
          local.get 0
          local.get 4
          i32.store offset=4
          local.get 0
          local.get 6
          i32.store
          local.get 0
          i32.const 8
          i32.add
          local.get 2
          i32.store
          local.get 3
          i32.const 80
          i32.add
          global.set 0
          return
        end
        call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$11allocate_in28_$u7b$$u7b$closure$u7d$$u7d$17h0fefaba762b2b53dE
        unreachable
      end
      local.get 1
      i32.const 0
      call $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E
      unreachable
    end
    local.get 1
    i32.const 1
    call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
    unreachable)
  (func $_ZN3std3sys4wasi11unsupported17h259482e01b7d0f2aE (type 0) (param i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 1
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          i32.const 35
          i32.const 1
          call $__rust_alloc
          local.tee 2
          i32.eqz
          br_if 0 (;@3;)
          local.get 2
          i32.const 31
          i32.add
          i32.const 0
          i32.load offset=1052619 align=1
          i32.store align=1
          local.get 2
          i32.const 24
          i32.add
          i32.const 0
          i64.load offset=1052612 align=1
          i64.store align=1
          local.get 2
          i32.const 16
          i32.add
          i32.const 0
          i64.load offset=1052604 align=1
          i64.store align=1
          local.get 2
          i32.const 8
          i32.add
          i32.const 0
          i64.load offset=1052596 align=1
          i64.store align=1
          local.get 2
          i32.const 0
          i64.load offset=1052588 align=1
          i64.store align=1
          i32.const 12
          i32.const 4
          call $__rust_alloc
          local.tee 3
          i32.eqz
          br_if 1 (;@2;)
          local.get 3
          i64.const 150323855395
          i64.store offset=4 align=4
          local.get 3
          local.get 2
          i32.store
          i32.const 12
          i32.const 4
          call $__rust_alloc
          local.tee 2
          i32.eqz
          br_if 2 (;@1;)
          local.get 2
          i32.const 16
          i32.store8 offset=8
          local.get 2
          i32.const 1051084
          i32.store offset=4
          local.get 2
          local.get 3
          i32.store
          local.get 2
          local.get 1
          i32.load16_u offset=13 align=1
          i32.store16 offset=9 align=1
          local.get 2
          i32.const 11
          i32.add
          local.get 1
          i32.const 15
          i32.add
          i32.load8_u
          i32.store8
          local.get 0
          i32.const 8
          i32.add
          local.get 2
          i32.store
          local.get 0
          i64.const 8589934593
          i64.store align=4
          local.get 1
          i32.const 16
          i32.add
          global.set 0
          return
        end
        i32.const 35
        i32.const 1
        call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
        unreachable
      end
      i32.const 12
      i32.const 4
      call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
      unreachable
    end
    i32.const 12
    i32.const 4
    call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
    unreachable)
  (func $_ZN3std3ffi5c_str104_$LT$impl$u20$core..convert..From$LT$std..ffi..c_str..NulError$GT$$u20$for$u20$std..io..error..Error$GT$4from17hf5367864c69978b6E (type 4) (param i32 i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          i32.const 33
          i32.const 1
          call $__rust_alloc
          local.tee 3
          i32.eqz
          br_if 0 (;@3;)
          local.get 3
          i32.const 32
          i32.add
          i32.const 0
          i32.load8_u offset=1051157
          i32.store8
          local.get 3
          i32.const 24
          i32.add
          i32.const 0
          i64.load offset=1051149 align=1
          i64.store align=1
          local.get 3
          i32.const 16
          i32.add
          i32.const 0
          i64.load offset=1051141 align=1
          i64.store align=1
          local.get 3
          i32.const 8
          i32.add
          i32.const 0
          i64.load offset=1051133 align=1
          i64.store align=1
          local.get 3
          i32.const 0
          i64.load offset=1051125 align=1
          i64.store align=1
          i32.const 12
          i32.const 4
          call $__rust_alloc
          local.tee 4
          i32.eqz
          br_if 1 (;@2;)
          local.get 4
          i64.const 141733920801
          i64.store offset=4 align=4
          local.get 4
          local.get 3
          i32.store
          i32.const 12
          i32.const 4
          call $__rust_alloc
          local.tee 3
          i32.eqz
          br_if 2 (;@1;)
          local.get 3
          i32.const 11
          i32.store8 offset=8
          local.get 3
          i32.const 1051084
          i32.store offset=4
          local.get 3
          local.get 4
          i32.store
          local.get 3
          local.get 2
          i32.load16_u offset=13 align=1
          i32.store16 offset=9 align=1
          local.get 3
          i32.const 11
          i32.add
          local.get 2
          i32.const 13
          i32.add
          i32.const 2
          i32.add
          i32.load8_u
          i32.store8
          local.get 0
          i32.const 2
          i32.store8
          local.get 0
          local.get 2
          i32.load16_u offset=10 align=1
          i32.store16 offset=1 align=1
          local.get 0
          i32.const 3
          i32.add
          local.get 2
          i32.const 10
          i32.add
          i32.const 2
          i32.add
          i32.load8_u
          i32.store8
          local.get 0
          i32.const 4
          i32.add
          local.get 3
          i32.store
          block  ;; label = @4
            local.get 1
            i32.const 8
            i32.add
            i32.load
            local.tee 3
            i32.eqz
            br_if 0 (;@4;)
            local.get 1
            i32.load offset=4
            local.get 3
            i32.const 1
            call $__rust_dealloc
          end
          local.get 2
          i32.const 16
          i32.add
          global.set 0
          return
        end
        i32.const 33
        i32.const 1
        call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
        unreachable
      end
      i32.const 12
      i32.const 4
      call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
      unreachable
    end
    i32.const 12
    i32.const 4
    call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
    unreachable)
  (func $_ZN60_$LT$std..io..error..Error$u20$as$u20$core..fmt..Display$GT$3fmt17hfbf3380c56bbc0baE (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 2
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 0
            i32.load8_u
            br_table 0 (;@4;) 2 (;@2;) 1 (;@3;) 0 (;@4;)
          end
          local.get 2
          local.get 0
          i32.const 4
          i32.add
          i32.load
          local.tee 0
          i32.store offset=4
          local.get 2
          i32.const 8
          i32.add
          local.get 0
          call $_ZN3std3sys4wasi2os12error_string17h85d4fca823e580faE
          local.get 2
          i32.const 60
          i32.add
          i32.const 2
          i32.store
          local.get 2
          i32.const 36
          i32.add
          i32.const 25
          i32.store
          local.get 2
          i64.const 3
          i64.store offset=44 align=4
          local.get 2
          i32.const 1051476
          i32.store offset=40
          local.get 2
          i32.const 26
          i32.store offset=28
          local.get 2
          local.get 2
          i32.const 24
          i32.add
          i32.store offset=56
          local.get 2
          local.get 2
          i32.const 4
          i32.add
          i32.store offset=32
          local.get 2
          local.get 2
          i32.const 8
          i32.add
          i32.store offset=24
          local.get 1
          local.get 2
          i32.const 40
          i32.add
          call $_ZN4core3fmt9Formatter9write_fmt17ha552aa6bb1a0a03bE
          local.set 0
          local.get 2
          i32.load offset=12
          local.tee 1
          i32.eqz
          br_if 2 (;@1;)
          local.get 2
          i32.load offset=8
          local.get 1
          i32.const 1
          call $__rust_dealloc
          br 2 (;@1;)
        end
        local.get 0
        i32.const 4
        i32.add
        i32.load
        local.tee 0
        i32.load
        local.get 1
        local.get 0
        i32.load offset=4
        i32.load offset=32
        call_indirect (type 2)
        local.set 0
        br 1 (;@1;)
      end
      i32.const 1051158
      local.set 3
      i32.const 22
      local.set 4
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      block  ;; label = @10
                        block  ;; label = @11
                          block  ;; label = @12
                            block  ;; label = @13
                              block  ;; label = @14
                                block  ;; label = @15
                                  block  ;; label = @16
                                    block  ;; label = @17
                                      block  ;; label = @18
                                        block  ;; label = @19
                                          block  ;; label = @20
                                            local.get 0
                                            i32.load8_u offset=1
                                            br_table 0 (;@20;) 1 (;@19;) 2 (;@18;) 3 (;@17;) 4 (;@16;) 5 (;@15;) 6 (;@14;) 7 (;@13;) 8 (;@12;) 9 (;@11;) 10 (;@10;) 11 (;@9;) 12 (;@8;) 13 (;@7;) 14 (;@6;) 15 (;@5;) 16 (;@4;) 18 (;@2;) 0 (;@20;)
                                          end
                                          i32.const 1051439
                                          local.set 3
                                          i32.const 16
                                          local.set 4
                                          br 17 (;@2;)
                                        end
                                        i32.const 1051422
                                        local.set 3
                                        i32.const 17
                                        local.set 4
                                        br 16 (;@2;)
                                      end
                                      i32.const 1051404
                                      local.set 3
                                      i32.const 18
                                      local.set 4
                                      br 15 (;@2;)
                                    end
                                    i32.const 1051388
                                    local.set 3
                                    i32.const 16
                                    local.set 4
                                    br 14 (;@2;)
                                  end
                                  i32.const 1051370
                                  local.set 3
                                  i32.const 18
                                  local.set 4
                                  br 13 (;@2;)
                                end
                                i32.const 1051357
                                local.set 3
                                i32.const 13
                                local.set 4
                                br 12 (;@2;)
                              end
                              i32.const 1051343
                              local.set 3
                              br 10 (;@3;)
                            end
                            i32.const 1051322
                            local.set 3
                            i32.const 21
                            local.set 4
                            br 10 (;@2;)
                          end
                          i32.const 1051311
                          local.set 3
                          i32.const 11
                          local.set 4
                          br 9 (;@2;)
                        end
                        i32.const 1051290
                        local.set 3
                        i32.const 21
                        local.set 4
                        br 8 (;@2;)
                      end
                      i32.const 1051269
                      local.set 3
                      i32.const 21
                      local.set 4
                      br 7 (;@2;)
                    end
                    i32.const 1051246
                    local.set 3
                    i32.const 23
                    local.set 4
                    br 6 (;@2;)
                  end
                  i32.const 1051234
                  local.set 3
                  i32.const 12
                  local.set 4
                  br 5 (;@2;)
                end
                i32.const 1051225
                local.set 3
                i32.const 9
                local.set 4
                br 4 (;@2;)
              end
              i32.const 1051215
              local.set 3
              i32.const 10
              local.set 4
              br 3 (;@2;)
            end
            i32.const 1051194
            local.set 3
            i32.const 21
            local.set 4
            br 2 (;@2;)
          end
          i32.const 1051180
          local.set 3
        end
        i32.const 14
        local.set 4
      end
      local.get 2
      i32.const 60
      i32.add
      i32.const 1
      i32.store
      local.get 2
      local.get 4
      i32.store offset=28
      local.get 2
      local.get 3
      i32.store offset=24
      local.get 2
      i32.const 27
      i32.store offset=12
      local.get 2
      i64.const 1
      i64.store offset=44 align=4
      local.get 2
      i32.const 1051456
      i32.store offset=40
      local.get 2
      local.get 2
      i32.const 24
      i32.add
      i32.store offset=8
      local.get 2
      local.get 2
      i32.const 8
      i32.add
      i32.store offset=56
      local.get 1
      local.get 2
      i32.const 40
      i32.add
      call $_ZN4core3fmt9Formatter9write_fmt17ha552aa6bb1a0a03bE
      local.set 0
    end
    local.get 2
    i32.const 64
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN55_$LT$std..path..Display$u20$as$u20$core..fmt..Debug$GT$3fmt17h669142b7b6fe9e0fE (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 0
    i32.load offset=4
    local.get 1
    call $_ZN73_$LT$std..sys_common..os_str_bytes..Slice$u20$as$u20$core..fmt..Debug$GT$3fmt17h8b2c8a7213186b17E)
  (func $_ZN3std5error5Error7type_id17h64ce692d4319812fE (type 1) (param i32) (result i64)
    i64.const -32900538044362730)
  (func $_ZN3std5error5Error9backtrace17h799bc3f420176d06E (type 14) (param i32) (result i32)
    i32.const 0)
  (func $_ZN3std5error5Error5cause17h955b446571fa3458E (type 4) (param i32 i32)
    local.get 0
    i32.const 0
    i32.store)
  (func $_ZN243_$LT$std..error..$LT$impl$u20$core..convert..From$LT$alloc..string..String$GT$$u20$for$u20$alloc..boxed..Box$LT$dyn$u20$std..error..Error$u2b$core..marker..Sync$u2b$core..marker..Send$GT$$GT$..from..StringError$u20$as$u20$std..error..Error$GT$11description17hbaa389d79ee7d499E (type 4) (param i32 i32)
    local.get 0
    local.get 1
    i32.load offset=8
    i32.store offset=4
    local.get 0
    local.get 1
    i32.load
    i32.store)
  (func $_ZN244_$LT$std..error..$LT$impl$u20$core..convert..From$LT$alloc..string..String$GT$$u20$for$u20$alloc..boxed..Box$LT$dyn$u20$std..error..Error$u2b$core..marker..Sync$u2b$core..marker..Send$GT$$GT$..from..StringError$u20$as$u20$core..fmt..Display$GT$3fmt17h9ecb4409df561c37E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 0
    i32.load offset=8
    local.get 1
    call $_ZN42_$LT$str$u20$as$u20$core..fmt..Display$GT$3fmt17hcf977a4d08f25cc0E)
  (func $_ZN242_$LT$std..error..$LT$impl$u20$core..convert..From$LT$alloc..string..String$GT$$u20$for$u20$alloc..boxed..Box$LT$dyn$u20$std..error..Error$u2b$core..marker..Sync$u2b$core..marker..Send$GT$$GT$..from..StringError$u20$as$u20$core..fmt..Debug$GT$3fmt17h672b3228d831bc08E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 0
    i32.load offset=8
    local.get 1
    call $_ZN40_$LT$str$u20$as$u20$core..fmt..Debug$GT$3fmt17h1a54fd8ecae06fe9E)
  (func $_ZN3std3sys4wasi17decode_error_kind17h45e11c33b7590f34E (type 14) (param i32) (result i32)
    (local i32)
    i32.const 16
    local.set 1
    block  ;; label = @1
      local.get 0
      i32.const 65535
      i32.gt_u
      br_if 0 (;@1;)
      local.get 0
      i32.const 65535
      i32.and
      i32.const -2
      i32.add
      local.tee 0
      i32.const 71
      i32.gt_u
      br_if 0 (;@1;)
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      block  ;; label = @10
                        block  ;; label = @11
                          block  ;; label = @12
                            block  ;; label = @13
                              block  ;; label = @14
                                block  ;; label = @15
                                  local.get 0
                                  br_table 2 (;@13;) 7 (;@8;) 6 (;@9;) 14 (;@1;) 13 (;@2;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 5 (;@10;) 0 (;@15;) 1 (;@14;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 12 (;@3;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 9 (;@6;) 10 (;@5;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 8 (;@7;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 4 (;@11;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 2 (;@13;) 3 (;@12;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 14 (;@1;) 11 (;@4;) 2 (;@13;)
                                end
                                i32.const 2
                                return
                              end
                              i32.const 3
                              return
                            end
                            i32.const 1
                            return
                          end
                          i32.const 8
                          return
                        end
                        i32.const 5
                        return
                      end
                      i32.const 4
                      return
                    end
                    i32.const 7
                    return
                  end
                  i32.const 6
                  return
                end
                i32.const 0
                return
              end
              i32.const 15
              return
            end
            i32.const 11
            return
          end
          i32.const 13
          return
        end
        i32.const 9
        return
      end
      i32.const 10
      local.set 1
    end
    local.get 1)
  (func $_ZN3std3sys4wasi2os12error_string17h85d4fca823e580faE (type 4) (param i32 i32)
    (local i32 i32 i32 i32)
    global.get 0
    i32.const 1056
    i32.sub
    local.tee 2
    global.set 0
    i32.const 0
    local.set 3
    local.get 2
    i32.const 8
    i32.add
    i32.const 0
    i32.const 1024
    call $memset
    drop
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              local.get 1
              local.get 2
              i32.const 8
              i32.add
              i32.const 1024
              call $strerror_r
              i32.const 0
              i32.lt_s
              br_if 0 (;@5;)
              block  ;; label = @6
                local.get 2
                i32.load8_u offset=8
                i32.eqz
                br_if 0 (;@6;)
                local.get 2
                i32.const 8
                i32.add
                i32.const 1
                i32.add
                local.set 4
                i32.const 0
                local.set 1
                loop  ;; label = @7
                  local.get 4
                  local.get 1
                  i32.add
                  local.set 5
                  local.get 1
                  i32.const 1
                  i32.add
                  local.tee 3
                  local.set 1
                  local.get 5
                  i32.load8_u
                  br_if 0 (;@7;)
                end
                local.get 3
                i32.const -1
                i32.eq
                br_if 2 (;@4;)
              end
              local.get 2
              i32.const 1032
              i32.add
              local.get 2
              i32.const 8
              i32.add
              local.get 3
              call $_ZN4core3str9from_utf817h40c83401242cc090E
              local.get 2
              i32.load offset=1032
              i32.const 1
              i32.eq
              br_if 2 (;@3;)
              local.get 2
              i32.const 1040
              i32.add
              i32.load
              local.tee 1
              i32.const -1
              i32.le_s
              br_if 3 (;@2;)
              local.get 2
              i32.load offset=1036
              local.set 5
              block  ;; label = @6
                block  ;; label = @7
                  local.get 1
                  br_if 0 (;@7;)
                  i32.const 1
                  local.set 3
                  br 1 (;@6;)
                end
                local.get 1
                i32.const 1
                call $__rust_alloc
                local.tee 3
                i32.eqz
                br_if 5 (;@1;)
              end
              local.get 3
              local.get 5
              local.get 1
              call $memcpy
              local.set 5
              local.get 0
              local.get 1
              i32.store offset=8
              local.get 0
              local.get 1
              i32.store offset=4
              local.get 0
              local.get 5
              i32.store
              local.get 2
              i32.const 1056
              i32.add
              global.set 0
              return
            end
            i32.const 1052476
            i32.const 18
            i32.const 1052520
            call $_ZN3std9panicking11begin_panic17h4f03e37f2089bbdaE
            unreachable
          end
          local.get 3
          i32.const 0
          call $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E
          unreachable
        end
        local.get 2
        local.get 2
        i64.load offset=1036 align=4
        i64.store offset=1048
        i32.const 1050648
        i32.const 43
        local.get 2
        i32.const 1048
        i32.add
        i32.const 1050692
        i32.const 1052536
        call $_ZN4core6option18expect_none_failed17h5718e8afd751d0acE
        unreachable
      end
      call $_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$11allocate_in28_$u7b$$u7b$closure$u7d$$u7d$17h0fefaba762b2b53dE
      unreachable
    end
    local.get 1
    i32.const 1
    call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
    unreachable)
  (func $_ZN3std2io5impls71_$LT$impl$u20$std..io..Write$u20$for$u20$alloc..boxed..Box$LT$W$GT$$GT$5write17h3f92adeca71419b2E (type 3) (param i32 i32 i32 i32)
    local.get 0
    local.get 1
    i32.load
    local.get 2
    local.get 3
    local.get 1
    i32.load offset=4
    i32.load offset=12
    call_indirect (type 3))
  (func $_ZN3std2io5impls71_$LT$impl$u20$std..io..Write$u20$for$u20$alloc..boxed..Box$LT$W$GT$$GT$14write_vectored17hfc1c93a246624d48E (type 3) (param i32 i32 i32 i32)
    local.get 0
    local.get 1
    i32.load
    local.get 2
    local.get 3
    local.get 1
    i32.load offset=4
    i32.load offset=16
    call_indirect (type 3))
  (func $_ZN3std2io5impls71_$LT$impl$u20$std..io..Write$u20$for$u20$alloc..boxed..Box$LT$W$GT$$GT$5flush17ha92b9fce67924e8eE (type 4) (param i32 i32)
    local.get 0
    local.get 1
    i32.load
    local.get 1
    i32.load offset=4
    i32.load offset=20
    call_indirect (type 4))
  (func $_ZN3std2io5impls71_$LT$impl$u20$std..io..Write$u20$for$u20$alloc..boxed..Box$LT$W$GT$$GT$9write_all17h58e4711e602e748fE (type 3) (param i32 i32 i32 i32)
    local.get 0
    local.get 1
    i32.load
    local.get 2
    local.get 3
    local.get 1
    i32.load offset=4
    i32.load offset=24
    call_indirect (type 3))
  (func $_ZN3std2io5impls71_$LT$impl$u20$std..io..Write$u20$for$u20$alloc..boxed..Box$LT$W$GT$$GT$9write_fmt17hed5dbf8ed70707e1E (type 5) (param i32 i32 i32)
    (local i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 3
    global.set 0
    local.get 1
    i32.load
    local.set 4
    local.get 1
    i32.load offset=4
    local.set 1
    local.get 3
    i32.const 8
    i32.add
    i32.const 16
    i32.add
    local.get 2
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 3
    i32.const 8
    i32.add
    i32.const 8
    i32.add
    local.get 2
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 3
    local.get 2
    i64.load align=4
    i64.store offset=8
    local.get 0
    local.get 4
    local.get 3
    i32.const 8
    i32.add
    local.get 1
    i32.load offset=28
    call_indirect (type 5)
    local.get 3
    i32.const 32
    i32.add
    global.set 0)
  (func $_ZN60_$LT$std..io..stdio..StderrRaw$u20$as$u20$std..io..Write$GT$5write17hd1489a4d8b578e60E (type 3) (param i32 i32 i32 i32)
    (local i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 4
    global.set 0
    local.get 4
    local.get 3
    i32.store offset=12
    local.get 4
    local.get 2
    i32.store offset=8
    i32.const 1
    local.set 2
    local.get 4
    i32.const 16
    i32.add
    i32.const 2
    local.get 4
    i32.const 8
    i32.add
    i32.const 1
    call $_ZN4wasi13lib_generated8fd_write17h599f0274fd7b3c57E
    block  ;; label = @1
      block  ;; label = @2
        local.get 4
        i32.load16_u offset=16
        i32.const 1
        i32.eq
        br_if 0 (;@2;)
        local.get 0
        local.get 4
        i32.load offset=20
        i32.store offset=4
        i32.const 0
        local.set 2
        br 1 (;@1;)
      end
      local.get 4
      local.get 4
      i32.load16_u offset=18
      i32.store16 offset=30
      local.get 0
      local.get 4
      i32.const 30
      i32.add
      call $_ZN4wasi5error5Error9raw_error17h3d77f281c47bf703E
      i64.extend_i32_u
      i64.const 65535
      i64.and
      i64.const 32
      i64.shl
      i64.store offset=4 align=4
    end
    local.get 0
    local.get 2
    i32.store
    local.get 4
    i32.const 32
    i32.add
    global.set 0)
  (func $_ZN3std2io5stdio9set_panic17h18c62f96637563b7E (type 5) (param i32 i32 i32)
    (local i32 i32 i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 3
    global.set 0
    i32.const 0
    local.set 4
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          i32.const 0
          i32.load offset=1059004
          i32.const 1
          i32.eq
          br_if 0 (;@3;)
          i32.const 0
          i64.const 1
          i64.store offset=1059004 align=4
          i32.const 0
          i32.const 0
          i32.store offset=1059012
          br 1 (;@2;)
        end
        i32.const 0
        i32.load offset=1059008
        br_if 1 (;@1;)
        i32.const 0
        i32.load offset=1059012
        local.set 4
      end
      i32.const 0
      local.get 1
      i32.store offset=1059012
      i32.const 0
      i32.load offset=1059016
      local.set 1
      i32.const 0
      local.get 2
      i32.store offset=1059016
      i32.const 0
      i32.const 0
      i32.store offset=1059008
      block  ;; label = @2
        local.get 4
        i32.eqz
        br_if 0 (;@2;)
        local.get 3
        local.get 4
        local.get 1
        i32.load offset=20
        call_indirect (type 4)
        block  ;; label = @3
          i32.const 0
          br_if 0 (;@3;)
          local.get 3
          i32.load8_u
          i32.const 2
          i32.ne
          br_if 1 (;@2;)
        end
        local.get 3
        i32.load offset=4
        local.tee 2
        i32.load
        local.get 2
        i32.load offset=4
        i32.load
        call_indirect (type 0)
        block  ;; label = @3
          local.get 2
          i32.load offset=4
          local.tee 5
          i32.load offset=4
          local.tee 6
          i32.eqz
          br_if 0 (;@3;)
          local.get 2
          i32.load
          local.get 6
          local.get 5
          i32.load offset=8
          call $__rust_dealloc
        end
        local.get 2
        i32.const 12
        i32.const 4
        call $__rust_dealloc
      end
      local.get 0
      local.get 4
      i32.store
      local.get 0
      local.get 1
      i32.store offset=4
      local.get 3
      i32.const 16
      i32.add
      global.set 0
      return
    end
    i32.const 1050396
    i32.const 16
    local.get 3
    i32.const 8
    i32.add
    i32.const 1050600
    i32.const 1050480
    call $_ZN4core6option18expect_none_failed17h5718e8afd751d0acE
    unreachable)
  (func $_ZN3std2io5Write14write_vectored17h08df67e913c2a8b3E (type 3) (param i32 i32 i32 i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 4
    global.set 0
    local.get 3
    i32.const 3
    i32.shl
    local.set 3
    local.get 2
    i32.const -8
    i32.add
    local.set 5
    block  ;; label = @1
      loop  ;; label = @2
        block  ;; label = @3
          local.get 3
          br_if 0 (;@3;)
          i32.const 1050536
          local.set 2
          i32.const 0
          local.set 6
          br 2 (;@1;)
        end
        local.get 3
        i32.const -8
        i32.add
        local.set 3
        local.get 5
        i32.const 8
        i32.add
        local.set 5
        local.get 2
        i32.load offset=4
        local.set 6
        local.get 2
        i32.const 8
        i32.add
        local.set 2
        local.get 6
        i32.eqz
        br_if 0 (;@2;)
      end
      local.get 5
      i32.load
      local.set 2
    end
    local.get 4
    local.get 6
    i32.store offset=12
    local.get 4
    local.get 2
    i32.store offset=8
    i32.const 1
    local.set 2
    local.get 4
    i32.const 16
    i32.add
    i32.const 2
    local.get 4
    i32.const 8
    i32.add
    i32.const 1
    call $_ZN4wasi13lib_generated8fd_write17h599f0274fd7b3c57E
    block  ;; label = @1
      block  ;; label = @2
        local.get 4
        i32.load16_u offset=16
        i32.const 1
        i32.eq
        br_if 0 (;@2;)
        local.get 0
        local.get 4
        i32.load offset=20
        i32.store offset=4
        i32.const 0
        local.set 2
        br 1 (;@1;)
      end
      local.get 4
      local.get 4
      i32.load16_u offset=18
      i32.store16 offset=30
      local.get 0
      local.get 4
      i32.const 30
      i32.add
      call $_ZN4wasi5error5Error9raw_error17h3d77f281c47bf703E
      i64.extend_i32_u
      i64.const 65535
      i64.and
      i64.const 32
      i64.shl
      i64.store offset=4 align=4
    end
    local.get 0
    local.get 2
    i32.store
    local.get 4
    i32.const 32
    i32.add
    global.set 0)
  (func $_ZN3std2io5Write9write_fmt17h5ee903061f640461E (type 5) (param i32 i32 i32)
    (local i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    i32.const 3
    i32.store8 offset=12
    local.get 3
    local.get 1
    i32.store offset=8
    local.get 3
    i32.const 24
    i32.add
    i32.const 16
    i32.add
    local.get 2
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 3
    i32.const 24
    i32.add
    i32.const 8
    i32.add
    local.get 2
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 3
    local.get 2
    i64.load align=4
    i64.store offset=24
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              local.get 3
              i32.const 8
              i32.add
              i32.const 1051528
              local.get 3
              i32.const 24
              i32.add
              call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
              i32.eqz
              br_if 0 (;@5;)
              block  ;; label = @6
                local.get 3
                i32.load8_u offset=12
                i32.const 3
                i32.ne
                br_if 0 (;@6;)
                i32.const 15
                i32.const 1
                call $__rust_alloc
                local.tee 2
                i32.eqz
                br_if 2 (;@4;)
                local.get 2
                i32.const 7
                i32.add
                i32.const 0
                i64.load offset=1051559 align=1
                i64.store align=1
                local.get 2
                i32.const 0
                i64.load offset=1051552 align=1
                i64.store align=1
                i32.const 12
                i32.const 4
                call $__rust_alloc
                local.tee 1
                i32.eqz
                br_if 3 (;@3;)
                local.get 1
                i64.const 64424509455
                i64.store offset=4 align=4
                local.get 1
                local.get 2
                i32.store
                i32.const 12
                i32.const 4
                call $__rust_alloc
                local.tee 2
                i32.eqz
                br_if 4 (;@2;)
                local.get 2
                i32.const 16
                i32.store8 offset=8
                local.get 2
                i32.const 1051084
                i32.store offset=4
                local.get 2
                local.get 1
                i32.store
                local.get 2
                local.get 3
                i32.load16_u offset=24 align=1
                i32.store16 offset=9 align=1
                local.get 2
                i32.const 11
                i32.add
                local.get 3
                i32.const 24
                i32.add
                i32.const 2
                i32.add
                i32.load8_u
                i32.store8
                local.get 0
                i32.const 4
                i32.add
                local.get 2
                i32.store
                local.get 0
                i32.const 2
                i32.store
                br 5 (;@1;)
              end
              local.get 0
              local.get 3
              i64.load offset=12 align=4
              i64.store align=4
              br 4 (;@1;)
            end
            local.get 0
            i32.const 3
            i32.store8
            block  ;; label = @5
              i32.const 0
              br_if 0 (;@5;)
              local.get 3
              i32.load8_u offset=12
              i32.const 2
              i32.ne
              br_if 4 (;@1;)
            end
            local.get 3
            i32.const 16
            i32.add
            i32.load
            local.tee 2
            i32.load
            local.get 2
            i32.load offset=4
            i32.load
            call_indirect (type 0)
            block  ;; label = @5
              local.get 2
              i32.load offset=4
              local.tee 0
              i32.load offset=4
              local.tee 1
              i32.eqz
              br_if 0 (;@5;)
              local.get 2
              i32.load
              local.get 1
              local.get 0
              i32.load offset=8
              call $__rust_dealloc
            end
            local.get 3
            i32.load offset=16
            i32.const 12
            i32.const 4
            call $__rust_dealloc
            br 3 (;@1;)
          end
          i32.const 15
          i32.const 1
          call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
          unreachable
        end
        i32.const 12
        i32.const 4
        call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
        unreachable
      end
      i32.const 12
      i32.const 4
      call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
      unreachable
    end
    local.get 3
    i32.const 48
    i32.add
    global.set 0)
  (func $_ZN80_$LT$std..io..Write..write_fmt..Adaptor$LT$T$GT$$u20$as$u20$core..fmt..Write$GT$9write_str17h577dfb1621f2139fE (type 6) (param i32 i32 i32) (result i32)
    (local i32 i64 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    i32.const 8
    i32.add
    local.get 0
    i32.load
    local.get 1
    local.get 2
    call $_ZN3std2io5Write9write_all17ha85cbf0a744d542bE
    i32.const 0
    local.set 1
    block  ;; label = @1
      local.get 3
      i32.load8_u offset=8
      i32.const 3
      i32.eq
      br_if 0 (;@1;)
      local.get 3
      i64.load offset=8
      local.set 4
      block  ;; label = @2
        block  ;; label = @3
          i32.const 0
          br_if 0 (;@3;)
          local.get 0
          i32.load8_u offset=4
          i32.const 2
          i32.ne
          br_if 1 (;@2;)
        end
        local.get 0
        i32.const 8
        i32.add
        i32.load
        local.tee 1
        i32.load
        local.get 1
        i32.load offset=4
        i32.load
        call_indirect (type 0)
        block  ;; label = @3
          local.get 1
          i32.load offset=4
          local.tee 2
          i32.load offset=4
          local.tee 5
          i32.eqz
          br_if 0 (;@3;)
          local.get 1
          i32.load
          local.get 5
          local.get 2
          i32.load offset=8
          call $__rust_dealloc
        end
        local.get 0
        i32.load offset=8
        i32.const 12
        i32.const 4
        call $__rust_dealloc
      end
      local.get 0
      local.get 4
      i64.store offset=4 align=4
      i32.const 1
      local.set 1
    end
    local.get 3
    i32.const 16
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN59_$LT$std..process..ChildStdin$u20$as$u20$std..io..Write$GT$5flush17h9f125fd478950081E (type 4) (param i32 i32)
    local.get 0
    i32.const 3
    i32.store8)
  (func $_ZN3std7process5abort17h1646aa60de17f512E (type 9)
    call $_ZN3std3sys4wasi14abort_internal17ha90a87ebf5d74a1aE
    unreachable)
  (func $_ZN3std3sys4wasi14abort_internal17ha90a87ebf5d74a1aE (type 9)
    call $abort
    unreachable)
  (func $_ZN91_$LT$std..sys_common..backtrace.._print..DisplayBacktrace$u20$as$u20$core..fmt..Display$GT$3fmt17hefefb5408add7eddE (type 2) (param i32 i32) (result i32)
    (local i32 i64 i32 i32 i32 i32)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 2
    global.set 0
    local.get 0
    i32.load8_u
    local.set 0
    local.get 2
    i32.const 40
    i32.add
    call $_ZN3std3sys4wasi11unsupported17h259482e01b7d0f2aE
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.load offset=40
        i32.const 1
        i32.eq
        br_if 0 (;@2;)
        local.get 2
        i32.const 48
        i32.add
        i64.load
        local.set 3
        local.get 2
        i32.load offset=44
        local.set 4
        br 1 (;@1;)
      end
      i32.const 0
      local.set 4
      block  ;; label = @2
        local.get 2
        i32.load8_u offset=44
        i32.const 2
        i32.lt_u
        br_if 0 (;@2;)
        local.get 2
        i32.const 48
        i32.add
        i32.load
        local.tee 5
        i32.load
        local.get 5
        i32.load offset=4
        i32.load
        call_indirect (type 0)
        block  ;; label = @3
          local.get 5
          i32.load offset=4
          local.tee 6
          i32.load offset=4
          local.tee 7
          i32.eqz
          br_if 0 (;@3;)
          local.get 5
          i32.load
          local.get 7
          local.get 6
          i32.load offset=8
          call $__rust_dealloc
        end
        local.get 5
        i32.const 12
        i32.const 4
        call $__rust_dealloc
      end
    end
    local.get 2
    local.get 3
    i64.store offset=4 align=4
    local.get 2
    local.get 4
    i32.store
    local.get 2
    local.get 0
    i32.store8 offset=12
    local.get 2
    i32.const 16
    i32.add
    local.get 1
    local.get 0
    local.get 2
    i32.const 1051568
    call $_ZN9backtrace5print12BacktraceFmt3new17had3bff9dc6727220E
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.const 16
        i32.add
        call $_ZN9backtrace5print12BacktraceFmt11add_context17h0da2f7f19088e8c2E
        br_if 0 (;@2;)
        local.get 2
        i32.const 16
        i32.add
        call $_ZN9backtrace5print12BacktraceFmt6finish17hec231066b8ef0ca6E
        br_if 0 (;@2;)
        block  ;; label = @3
          local.get 0
          i32.const 255
          i32.and
          br_if 0 (;@3;)
          local.get 2
          i32.const 60
          i32.add
          i32.const 0
          i32.store
          local.get 2
          i32.const 1050536
          i32.store offset=56
          local.get 2
          i64.const 1
          i64.store offset=44 align=4
          local.get 2
          i32.const 1051676
          i32.store offset=40
          local.get 1
          local.get 2
          i32.const 40
          i32.add
          call $_ZN4core3fmt9Formatter9write_fmt17ha552aa6bb1a0a03bE
          br_if 1 (;@2;)
        end
        i32.const 0
        local.set 0
        local.get 2
        i32.load
        local.tee 1
        i32.eqz
        br_if 1 (;@1;)
        local.get 2
        i32.load offset=4
        local.tee 4
        i32.eqz
        br_if 1 (;@1;)
        local.get 1
        local.get 4
        i32.const 1
        call $__rust_dealloc
        br 1 (;@1;)
      end
      i32.const 1
      local.set 0
      local.get 2
      i32.load
      local.tee 1
      i32.eqz
      br_if 0 (;@1;)
      local.get 2
      i32.load offset=4
      local.tee 4
      i32.eqz
      br_if 0 (;@1;)
      i32.const 1
      local.set 0
      local.get 1
      local.get 4
      i32.const 1
      call $__rust_dealloc
    end
    local.get 2
    i32.const 64
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN3std10sys_common9backtrace10_print_fmt28_$u7b$$u7b$closure$u7d$$u7d$17hb8b7b5b7c7952fd5E (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 3
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.load
        i32.const 1
        i32.ne
        br_if 0 (;@2;)
        i32.const 1051688
        local.set 2
        i32.const 9
        local.set 4
        br 1 (;@1;)
      end
      local.get 3
      i32.const 16
      i32.add
      local.get 2
      i32.load offset=4
      local.get 2
      i32.const 8
      i32.add
      i32.load
      call $_ZN4core3str9from_utf817h40c83401242cc090E
      i32.const 1051688
      local.get 3
      i32.load offset=20
      local.get 3
      i32.load offset=16
      i32.const 1
      i32.eq
      local.tee 4
      select
      local.set 2
      i32.const 9
      local.get 3
      i32.const 16
      i32.add
      i32.const 8
      i32.add
      i32.load
      local.get 4
      select
      local.set 4
    end
    local.get 3
    i32.const 8
    i32.add
    local.get 2
    local.get 4
    call $_ZN4core3str5lossy9Utf8Lossy10from_bytes17h1357f46792efee29E
    local.get 3
    i32.load offset=8
    local.get 3
    i32.load offset=12
    local.get 1
    call $_ZN66_$LT$core..str..lossy..Utf8Lossy$u20$as$u20$core..fmt..Display$GT$3fmt17haeaf8cd40e5faeccE
    local.set 2
    local.get 3
    i32.const 32
    i32.add
    global.set 0
    local.get 2)
  (func $_ZN3std10sys_common4util10dumb_print17hee0860f52bd6625dE (type 0) (param i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 1
    global.set 0
    local.get 1
    i32.const 16
    i32.add
    i32.const 16
    i32.add
    local.get 0
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 1
    i32.const 16
    i32.add
    i32.const 8
    i32.add
    local.get 0
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 1
    local.get 0
    i64.load align=4
    i64.store offset=16
    local.get 1
    i32.const 8
    i32.add
    local.get 1
    i32.const 40
    i32.add
    local.get 1
    i32.const 16
    i32.add
    call $_ZN3std2io5Write9write_fmt17h5ee903061f640461E
    block  ;; label = @1
      block  ;; label = @2
        i32.const 0
        br_if 0 (;@2;)
        local.get 1
        i32.load8_u offset=8
        i32.const 2
        i32.ne
        br_if 1 (;@1;)
      end
      local.get 1
      i32.load offset=12
      local.tee 0
      i32.load
      local.get 0
      i32.load offset=4
      i32.load
      call_indirect (type 0)
      block  ;; label = @2
        local.get 0
        i32.load offset=4
        local.tee 2
        i32.load offset=4
        local.tee 3
        i32.eqz
        br_if 0 (;@2;)
        local.get 0
        i32.load
        local.get 3
        local.get 2
        i32.load offset=8
        call $__rust_dealloc
      end
      local.get 0
      i32.const 12
      i32.const 4
      call $__rust_dealloc
    end
    local.get 1
    i32.const 48
    i32.add
    global.set 0)
  (func $_ZN3std10sys_common4util5abort17h63b3d178f71979abE (type 0) (param i32)
    (local i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 1
    global.set 0
    local.get 1
    i32.const 20
    i32.add
    i32.const 1
    i32.store
    local.get 1
    i64.const 2
    i64.store offset=4 align=4
    local.get 1
    i32.const 1051764
    i32.store
    local.get 1
    i32.const 5
    i32.store offset=28
    local.get 1
    local.get 0
    i32.store offset=24
    local.get 1
    local.get 1
    i32.const 24
    i32.add
    i32.store offset=16
    local.get 1
    call $_ZN3std10sys_common4util10dumb_print17hee0860f52bd6625dE
    call $_ZN3std3sys4wasi14abort_internal17ha90a87ebf5d74a1aE
    unreachable)
  (func $_ZN3std5alloc24default_alloc_error_hook17h38966f062fa7b248E (type 4) (param i32 i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    i32.const 18
    i32.store offset=12
    local.get 2
    local.get 0
    i32.store offset=20
    local.get 2
    local.get 2
    i32.const 20
    i32.add
    i32.store offset=8
    local.get 2
    i32.const 52
    i32.add
    i32.const 1
    i32.store
    local.get 2
    i64.const 2
    i64.store offset=36 align=4
    local.get 2
    i32.const 1051816
    i32.store offset=32
    local.get 2
    local.get 2
    i32.const 8
    i32.add
    i32.store offset=48
    local.get 2
    i32.const 24
    i32.add
    local.get 2
    i32.const 56
    i32.add
    local.get 2
    i32.const 32
    i32.add
    call $_ZN3std2io5Write9write_fmt17h5ee903061f640461E
    block  ;; label = @1
      block  ;; label = @2
        i32.const 0
        br_if 0 (;@2;)
        local.get 2
        i32.load8_u offset=24
        i32.const 2
        i32.ne
        br_if 1 (;@1;)
      end
      local.get 2
      i32.load offset=28
      local.tee 0
      i32.load
      local.get 0
      i32.load offset=4
      i32.load
      call_indirect (type 0)
      block  ;; label = @2
        local.get 0
        i32.load offset=4
        local.tee 3
        i32.load offset=4
        local.tee 4
        i32.eqz
        br_if 0 (;@2;)
        local.get 0
        i32.load
        local.get 4
        local.get 3
        i32.load offset=8
        call $__rust_dealloc
      end
      local.get 0
      i32.const 12
      i32.const 4
      call $__rust_dealloc
    end
    local.get 2
    i32.const 64
    i32.add
    global.set 0)
  (func $rust_oom (type 4) (param i32 i32)
    (local i32)
    local.get 0
    local.get 1
    i32.const 0
    i32.load offset=1058988
    local.tee 2
    i32.const 28
    local.get 2
    select
    call_indirect (type 4)
    call $_ZN3std3sys4wasi14abort_internal17ha90a87ebf5d74a1aE
    unreachable)
  (func $__rdl_alloc (type 2) (param i32 i32) (result i32)
    block  ;; label = @1
      local.get 1
      i32.const 8
      i32.gt_u
      br_if 0 (;@1;)
      local.get 1
      local.get 0
      i32.gt_u
      br_if 0 (;@1;)
      local.get 0
      call $malloc
      return
    end
    local.get 0
    local.get 1
    call $aligned_alloc)
  (func $__rdl_dealloc (type 5) (param i32 i32 i32)
    local.get 0
    call $free)
  (func $__rdl_realloc (type 8) (param i32 i32 i32 i32) (result i32)
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.const 8
        i32.gt_u
        br_if 0 (;@2;)
        local.get 2
        local.get 3
        i32.le_u
        br_if 1 (;@1;)
      end
      block  ;; label = @2
        local.get 3
        local.get 2
        call $aligned_alloc
        local.tee 2
        br_if 0 (;@2;)
        i32.const 0
        return
      end
      local.get 2
      local.get 0
      local.get 3
      local.get 1
      local.get 1
      local.get 3
      i32.gt_u
      select
      call $memcpy
      local.set 3
      local.get 0
      call $free
      local.get 3
      return
    end
    local.get 0
    local.get 3
    call $realloc)
  (func $_ZN3std9panicking12default_hook28_$u7b$$u7b$closure$u7d$$u7d$17hd5e959ee7cc2957bE (type 5) (param i32 i32 i32)
    (local i32 i32 i32 i32)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    i32.const 20
    i32.add
    i32.const 3
    i32.store
    local.get 3
    i32.const 32
    i32.add
    i32.const 20
    i32.add
    i32.const 29
    i32.store
    local.get 3
    i32.const 44
    i32.add
    i32.const 27
    i32.store
    local.get 3
    i64.const 4
    i64.store offset=4 align=4
    local.get 3
    i32.const 1051992
    i32.store
    local.get 3
    i32.const 27
    i32.store offset=36
    local.get 3
    local.get 0
    i32.load offset=8
    i32.store offset=48
    local.get 3
    local.get 0
    i32.load offset=4
    i32.store offset=40
    local.get 3
    local.get 0
    i32.load
    i32.store offset=32
    local.get 3
    local.get 3
    i32.const 32
    i32.add
    i32.store offset=16
    local.get 3
    i32.const 24
    i32.add
    local.get 1
    local.get 3
    local.get 2
    i32.load offset=28
    local.tee 2
    call_indirect (type 5)
    block  ;; label = @1
      block  ;; label = @2
        i32.const 0
        br_if 0 (;@2;)
        local.get 3
        i32.load8_u offset=24
        i32.const 2
        i32.ne
        br_if 1 (;@1;)
      end
      local.get 3
      i32.load offset=28
      local.tee 4
      i32.load
      local.get 4
      i32.load offset=4
      i32.load
      call_indirect (type 0)
      block  ;; label = @2
        local.get 4
        i32.load offset=4
        local.tee 5
        i32.load offset=4
        local.tee 6
        i32.eqz
        br_if 0 (;@2;)
        local.get 4
        i32.load
        local.get 6
        local.get 5
        i32.load offset=8
        call $__rust_dealloc
      end
      local.get 4
      i32.const 12
      i32.const 4
      call $__rust_dealloc
    end
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 0
            i32.load offset=12
            i32.load8_u
            local.tee 4
            i32.const -3
            i32.add
            i32.const 255
            i32.and
            local.tee 0
            i32.const 1
            i32.add
            i32.const 0
            local.get 0
            i32.const 2
            i32.lt_u
            select
            br_table 0 (;@4;) 2 (;@2;) 1 (;@3;) 0 (;@4;)
          end
          i32.const 0
          i32.load8_u offset=1059041
          br_if 2 (;@1;)
          i32.const 0
          i32.const 1
          i32.store8 offset=1059041
          local.get 3
          i32.const 52
          i32.add
          i32.const 1
          i32.store
          local.get 3
          i64.const 1
          i64.store offset=36 align=4
          local.get 3
          i32.const 1051456
          i32.store offset=32
          local.get 3
          i32.const 30
          i32.store offset=4
          local.get 3
          local.get 4
          i32.store8 offset=63
          local.get 3
          local.get 3
          i32.store offset=48
          local.get 3
          local.get 3
          i32.const 63
          i32.add
          i32.store
          local.get 3
          i32.const 24
          i32.add
          local.get 1
          local.get 3
          i32.const 32
          i32.add
          local.get 2
          call_indirect (type 5)
          i32.const 0
          i32.const 0
          i32.store8 offset=1059041
          block  ;; label = @4
            i32.const 0
            br_if 0 (;@4;)
            local.get 3
            i32.load8_u offset=24
            i32.const 2
            i32.ne
            br_if 2 (;@2;)
          end
          local.get 3
          i32.load offset=28
          local.tee 0
          i32.load
          local.get 0
          i32.load offset=4
          i32.load
          call_indirect (type 0)
          block  ;; label = @4
            local.get 0
            i32.load offset=4
            local.tee 1
            i32.load offset=4
            local.tee 2
            i32.eqz
            br_if 0 (;@4;)
            local.get 0
            i32.load
            local.get 2
            local.get 1
            i32.load offset=8
            call $__rust_dealloc
          end
          local.get 0
          i32.const 12
          i32.const 4
          call $__rust_dealloc
          br 1 (;@2;)
        end
        i32.const 0
        i32.load8_u offset=1058976
        local.set 0
        i32.const 0
        i32.const 0
        i32.store8 offset=1058976
        local.get 0
        i32.eqz
        br_if 0 (;@2;)
        local.get 3
        i32.const 52
        i32.add
        i32.const 0
        i32.store
        local.get 3
        i32.const 1050536
        i32.store offset=48
        local.get 3
        i64.const 1
        i64.store offset=36 align=4
        local.get 3
        i32.const 1052104
        i32.store offset=32
        local.get 3
        local.get 1
        local.get 3
        i32.const 32
        i32.add
        local.get 2
        call_indirect (type 5)
        block  ;; label = @3
          i32.const 0
          br_if 0 (;@3;)
          local.get 3
          i32.load8_u
          i32.const 2
          i32.ne
          br_if 1 (;@2;)
        end
        local.get 3
        i32.load offset=4
        local.tee 0
        i32.load
        local.get 0
        i32.load offset=4
        i32.load
        call_indirect (type 0)
        block  ;; label = @3
          local.get 0
          i32.load offset=4
          local.tee 1
          i32.load offset=4
          local.tee 2
          i32.eqz
          br_if 0 (;@3;)
          local.get 0
          i32.load
          local.get 2
          local.get 1
          i32.load offset=8
          call $__rust_dealloc
        end
        local.get 0
        i32.const 12
        i32.const 4
        call $__rust_dealloc
      end
      local.get 3
      i32.const 64
      i32.add
      global.set 0
      return
    end
    i32.const 1052392
    i32.const 32
    i32.const 1052460
    call $_ZN3std9panicking11begin_panic17h4f03e37f2089bbdaE
    unreachable)
  (func $rust_begin_unwind (type 0) (param i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 1
    global.set 0
    local.get 0
    call $_ZN4core5panic9PanicInfo8location17h30a49797d3e7ef56E
    i32.const 1052112
    call $_ZN4core6option15Option$LT$T$GT$6unwrap17h170a589b0290d63aE
    local.set 2
    local.get 0
    call $_ZN4core5panic9PanicInfo7message17hffa6f3d3e6ff0a39E
    call $_ZN4core6option15Option$LT$T$GT$6unwrap17h1739ecdc6eda48e2E
    local.set 3
    local.get 1
    i32.const 0
    i32.store offset=4
    local.get 1
    local.get 3
    i32.store
    local.get 1
    i32.const 1052144
    local.get 0
    call $_ZN4core5panic9PanicInfo7message17hffa6f3d3e6ff0a39E
    local.get 2
    call $_ZN3std9panicking20rust_panic_with_hook17h8bf13b9f643a54b1E
    unreachable)
  (func $_ZN3std9panicking20rust_panic_with_hook17h8bf13b9f643a54b1E (type 3) (param i32 i32 i32 i32)
    (local i32 i32)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 4
    global.set 0
    i32.const 1
    local.set 5
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            i32.const 0
            i32.load offset=1059032
            i32.const 1
            i32.eq
            br_if 0 (;@4;)
            i32.const 0
            i64.const 4294967297
            i64.store offset=1059032
            br 1 (;@3;)
          end
          i32.const 0
          i32.const 0
          i32.load offset=1059036
          i32.const 1
          i32.add
          local.tee 5
          i32.store offset=1059036
          local.get 5
          i32.const 2
          i32.gt_u
          br_if 1 (;@2;)
        end
        local.get 4
        local.get 3
        i32.store offset=36
        local.get 4
        local.get 2
        i32.store offset=32
        local.get 4
        i32.const 1050536
        i32.store offset=28
        local.get 4
        i32.const 1050536
        i32.store offset=24
        block  ;; label = @3
          i32.const 0
          i32.load offset=1058992
          local.tee 2
          i32.const -1
          i32.le_s
          br_if 0 (;@3;)
          i32.const 0
          local.get 2
          i32.const 1
          i32.add
          i32.store offset=1058992
          block  ;; label = @4
            block  ;; label = @5
              i32.const 0
              i32.load offset=1059000
              local.tee 2
              br_if 0 (;@5;)
              local.get 4
              i32.const 8
              i32.add
              local.get 0
              local.get 1
              i32.load offset=16
              call_indirect (type 4)
              local.get 4
              local.get 4
              i64.load offset=8
              i64.store offset=24
              local.get 4
              i32.const 24
              i32.add
              call $_ZN3std9panicking12default_hook17hfcbeaa3f98c73639E
              br 1 (;@4;)
            end
            i32.const 0
            i32.load offset=1058996
            local.set 3
            local.get 4
            i32.const 16
            i32.add
            local.get 0
            local.get 1
            i32.load offset=16
            call_indirect (type 4)
            local.get 4
            local.get 4
            i64.load offset=16
            i64.store offset=24
            local.get 3
            local.get 4
            i32.const 24
            i32.add
            local.get 2
            i32.load offset=12
            call_indirect (type 4)
          end
          i32.const 0
          i32.const 0
          i32.load offset=1058992
          i32.const -1
          i32.add
          i32.store offset=1058992
          local.get 5
          i32.const 1
          i32.le_u
          br_if 2 (;@1;)
          local.get 4
          i32.const 60
          i32.add
          i32.const 0
          i32.store
          local.get 4
          i32.const 1050536
          i32.store offset=56
          local.get 4
          i64.const 1
          i64.store offset=44 align=4
          local.get 4
          i32.const 1052320
          i32.store offset=40
          local.get 4
          i32.const 40
          i32.add
          call $_ZN3std10sys_common4util10dumb_print17hee0860f52bd6625dE
          unreachable
          unreachable
        end
        local.get 4
        i32.const 60
        i32.add
        i32.const 0
        i32.store
        local.get 4
        i32.const 1050536
        i32.store offset=56
        local.get 4
        i64.const 1
        i64.store offset=44 align=4
        local.get 4
        i32.const 1052580
        i32.store offset=40
        local.get 4
        i32.const 40
        i32.add
        call $_ZN3std10sys_common4util5abort17h63b3d178f71979abE
        unreachable
      end
      local.get 4
      i32.const 60
      i32.add
      i32.const 0
      i32.store
      local.get 4
      i32.const 1050536
      i32.store offset=56
      local.get 4
      i64.const 1
      i64.store offset=44 align=4
      local.get 4
      i32.const 1052268
      i32.store offset=40
      local.get 4
      i32.const 40
      i32.add
      call $_ZN3std10sys_common4util10dumb_print17hee0860f52bd6625dE
      unreachable
      unreachable
    end
    local.get 0
    local.get 1
    call $rust_panic
    unreachable)
  (func $_ZN90_$LT$std..panicking..begin_panic_handler..PanicPayload$u20$as$u20$core..panic..BoxMeUp$GT$8take_box17hf451b6db153646f3E (type 4) (param i32 i32)
    (local i32 i32 i32 i32 i32)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 2
    global.set 0
    block  ;; label = @1
      local.get 1
      i32.load offset=4
      local.tee 3
      br_if 0 (;@1;)
      local.get 1
      i32.const 4
      i32.add
      local.set 3
      local.get 1
      i32.load
      local.set 4
      local.get 2
      i32.const 0
      i32.store offset=32
      local.get 2
      i64.const 1
      i64.store offset=24
      local.get 2
      local.get 2
      i32.const 24
      i32.add
      i32.store offset=36
      local.get 2
      i32.const 40
      i32.add
      i32.const 16
      i32.add
      local.get 4
      i32.const 16
      i32.add
      i64.load align=4
      i64.store
      local.get 2
      i32.const 40
      i32.add
      i32.const 8
      i32.add
      local.get 4
      i32.const 8
      i32.add
      i64.load align=4
      i64.store
      local.get 2
      local.get 4
      i64.load align=4
      i64.store offset=40
      local.get 2
      i32.const 36
      i32.add
      i32.const 1050356
      local.get 2
      i32.const 40
      i32.add
      call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
      drop
      local.get 2
      i32.const 8
      i32.add
      i32.const 8
      i32.add
      local.tee 4
      local.get 2
      i32.load offset=32
      i32.store
      local.get 2
      local.get 2
      i64.load offset=24
      i64.store offset=8
      block  ;; label = @2
        local.get 1
        i32.load offset=4
        local.tee 5
        i32.eqz
        br_if 0 (;@2;)
        local.get 1
        i32.const 8
        i32.add
        i32.load
        local.tee 6
        i32.eqz
        br_if 0 (;@2;)
        local.get 5
        local.get 6
        i32.const 1
        call $__rust_dealloc
      end
      local.get 3
      local.get 2
      i64.load offset=8
      i64.store align=4
      local.get 3
      i32.const 8
      i32.add
      local.get 4
      i32.load
      i32.store
      local.get 3
      i32.load
      local.set 3
    end
    local.get 1
    i32.const 1
    i32.store offset=4
    local.get 1
    i32.const 12
    i32.add
    i32.load
    local.set 4
    local.get 1
    i32.const 8
    i32.add
    local.tee 1
    i32.load
    local.set 5
    local.get 1
    i64.const 0
    i64.store align=4
    block  ;; label = @1
      i32.const 12
      i32.const 4
      call $__rust_alloc
      local.tee 1
      br_if 0 (;@1;)
      i32.const 12
      i32.const 4
      call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
      unreachable
    end
    local.get 1
    local.get 4
    i32.store offset=8
    local.get 1
    local.get 5
    i32.store offset=4
    local.get 1
    local.get 3
    i32.store
    local.get 0
    i32.const 1052164
    i32.store offset=4
    local.get 0
    local.get 1
    i32.store
    local.get 2
    i32.const 64
    i32.add
    global.set 0)
  (func $_ZN90_$LT$std..panicking..begin_panic_handler..PanicPayload$u20$as$u20$core..panic..BoxMeUp$GT$3get17h0cf460ea902fc052E (type 4) (param i32 i32)
    (local i32 i32 i32 i32)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 2
    global.set 0
    local.get 1
    i32.const 4
    i32.add
    local.set 3
    block  ;; label = @1
      local.get 1
      i32.load offset=4
      br_if 0 (;@1;)
      local.get 1
      i32.load
      local.set 4
      local.get 2
      i32.const 0
      i32.store offset=32
      local.get 2
      i64.const 1
      i64.store offset=24
      local.get 2
      local.get 2
      i32.const 24
      i32.add
      i32.store offset=36
      local.get 2
      i32.const 40
      i32.add
      i32.const 16
      i32.add
      local.get 4
      i32.const 16
      i32.add
      i64.load align=4
      i64.store
      local.get 2
      i32.const 40
      i32.add
      i32.const 8
      i32.add
      local.get 4
      i32.const 8
      i32.add
      i64.load align=4
      i64.store
      local.get 2
      local.get 4
      i64.load align=4
      i64.store offset=40
      local.get 2
      i32.const 36
      i32.add
      i32.const 1050356
      local.get 2
      i32.const 40
      i32.add
      call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
      drop
      local.get 2
      i32.const 8
      i32.add
      i32.const 8
      i32.add
      local.tee 4
      local.get 2
      i32.load offset=32
      i32.store
      local.get 2
      local.get 2
      i64.load offset=24
      i64.store offset=8
      block  ;; label = @2
        local.get 1
        i32.load offset=4
        local.tee 5
        i32.eqz
        br_if 0 (;@2;)
        local.get 1
        i32.const 8
        i32.add
        i32.load
        local.tee 1
        i32.eqz
        br_if 0 (;@2;)
        local.get 5
        local.get 1
        i32.const 1
        call $__rust_dealloc
      end
      local.get 3
      local.get 2
      i64.load offset=8
      i64.store align=4
      local.get 3
      i32.const 8
      i32.add
      local.get 4
      i32.load
      i32.store
    end
    local.get 0
    i32.const 1052164
    i32.store offset=4
    local.get 0
    local.get 3
    i32.store
    local.get 2
    i32.const 64
    i32.add
    global.set 0)
  (func $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$8take_box17hb93df9b37cb795f9E (type 4) (param i32 i32)
    (local i32 i32)
    local.get 1
    i32.load
    local.set 2
    local.get 1
    i32.const 0
    i32.store
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.eqz
        br_if 0 (;@2;)
        local.get 1
        i32.load offset=4
        local.set 3
        i32.const 8
        i32.const 4
        call $__rust_alloc
        local.tee 1
        i32.eqz
        br_if 1 (;@1;)
        local.get 1
        local.get 3
        i32.store offset=4
        local.get 1
        local.get 2
        i32.store
        local.get 0
        i32.const 1052200
        i32.store offset=4
        local.get 0
        local.get 1
        i32.store
        return
      end
      call $_ZN3std7process5abort17h1646aa60de17f512E
      unreachable
    end
    i32.const 8
    i32.const 4
    call $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE
    unreachable)
  (func $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$3get17h5a8f37f29fff8575E (type 4) (param i32 i32)
    block  ;; label = @1
      local.get 1
      i32.load
      br_if 0 (;@1;)
      call $_ZN3std7process5abort17h1646aa60de17f512E
      unreachable
    end
    local.get 0
    i32.const 1052200
    i32.store offset=4
    local.get 0
    local.get 1
    i32.store)
  (func $rust_panic (type 4) (param i32 i32)
    (local i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 1
    i32.store offset=4
    local.get 2
    local.get 0
    i32.store
    local.get 2
    local.get 2
    call $__rust_start_panic
    i32.store offset=12
    local.get 2
    i32.const 36
    i32.add
    i32.const 1
    i32.store
    local.get 2
    i64.const 1
    i64.store offset=20 align=4
    local.get 2
    i32.const 1052360
    i32.store offset=16
    local.get 2
    i32.const 18
    i32.store offset=44
    local.get 2
    local.get 2
    i32.const 40
    i32.add
    i32.store offset=32
    local.get 2
    local.get 2
    i32.const 12
    i32.add
    i32.store offset=40
    local.get 2
    i32.const 16
    i32.add
    call $_ZN3std10sys_common4util5abort17h63b3d178f71979abE
    unreachable)
  (func $_ZN62_$LT$std..ffi..c_str..NulError$u20$as$u20$core..fmt..Debug$GT$3fmt17ha722d92d960e680dE (type 2) (param i32 i32) (result i32)
    (local i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 1
    i32.const 1052368
    i32.const 8
    call $_ZN4core3fmt9Formatter11debug_tuple17h1c7dc8aa00b962f9E
    local.get 2
    local.get 0
    i32.store offset=12
    local.get 2
    local.get 2
    i32.const 12
    i32.add
    i32.const 1050792
    call $_ZN4core3fmt8builders10DebugTuple5field17h95b19566bf4f9168E
    drop
    local.get 2
    local.get 0
    i32.const 4
    i32.add
    i32.store offset=12
    local.get 2
    local.get 2
    i32.const 12
    i32.add
    i32.const 1052376
    call $_ZN4core3fmt8builders10DebugTuple5field17h95b19566bf4f9168E
    drop
    local.get 2
    call $_ZN4core3fmt8builders10DebugTuple6finish17hd8ce6586f49c209fE
    local.set 0
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    local.get 0)
  (func $__rust_start_panic (type 14) (param i32) (result i32)
    unreachable
    unreachable)
  (func $_ZN4wasi5error5Error9raw_error17h3d77f281c47bf703E (type 14) (param i32) (result i32)
    local.get 0
    i32.load16_u)
  (func $_ZN4wasi13lib_generated8fd_write17h599f0274fd7b3c57E (type 3) (param i32 i32 i32 i32)
    (local i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 4
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        local.get 1
        local.get 2
        local.get 3
        local.get 4
        i32.const 12
        i32.add
        call $_ZN4wasi13lib_generated22wasi_snapshot_preview18fd_write17h62539b3299a4581fE
        local.tee 1
        br_if 0 (;@2;)
        local.get 0
        i32.const 4
        i32.add
        local.get 4
        i32.load offset=12
        i32.store
        i32.const 0
        local.set 1
        br 1 (;@1;)
      end
      local.get 0
      local.get 1
      i32.store16 offset=2
      i32.const 1
      local.set 1
    end
    local.get 0
    local.get 1
    i32.store16
    local.get 4
    i32.const 16
    i32.add
    global.set 0)
  (func $_ZN9backtrace5print12BacktraceFmt3new17had3bff9dc6727220E (type 12) (param i32 i32 i32 i32 i32)
    local.get 0
    local.get 2
    i32.store8 offset=16
    local.get 0
    i32.const 0
    i32.store offset=4
    local.get 0
    local.get 1
    i32.store
    local.get 0
    local.get 3
    i32.store offset=8
    local.get 0
    i32.const 12
    i32.add
    local.get 4
    i32.store)
  (func $_ZN9backtrace5print12BacktraceFmt11add_context17h0da2f7f19088e8c2E (type 14) (param i32) (result i32)
    local.get 0
    i32.load
    i32.const 1052623
    i32.const 17
    call $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E)
  (func $_ZN9backtrace5print12BacktraceFmt6finish17hec231066b8ef0ca6E (type 14) (param i32) (result i32)
    i32.const 0)
  (func $abort (type 9)
    unreachable
    unreachable)
  (func $malloc (type 14) (param i32) (result i32)
    local.get 0
    call $dlmalloc)
  (func $dlmalloc (type 14) (param i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 1
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      block  ;; label = @10
                        block  ;; label = @11
                          block  ;; label = @12
                            local.get 0
                            i32.const 236
                            i32.gt_u
                            br_if 0 (;@12;)
                            block  ;; label = @13
                              i32.const 0
                              i32.load offset=1059044
                              local.tee 2
                              i32.const 16
                              local.get 0
                              i32.const 19
                              i32.add
                              i32.const -16
                              i32.and
                              local.get 0
                              i32.const 11
                              i32.lt_u
                              select
                              local.tee 3
                              i32.const 3
                              i32.shr_u
                              local.tee 4
                              i32.shr_u
                              local.tee 0
                              i32.const 3
                              i32.and
                              i32.eqz
                              br_if 0 (;@13;)
                              local.get 0
                              i32.const 1
                              i32.and
                              local.get 4
                              i32.or
                              i32.const 1
                              i32.xor
                              local.tee 3
                              i32.const 3
                              i32.shl
                              local.tee 5
                              i32.const 1059092
                              i32.add
                              i32.load
                              local.tee 4
                              i32.const 8
                              i32.add
                              local.set 0
                              block  ;; label = @14
                                block  ;; label = @15
                                  local.get 4
                                  i32.load offset=8
                                  local.tee 6
                                  local.get 5
                                  i32.const 1059084
                                  i32.add
                                  local.tee 5
                                  i32.ne
                                  br_if 0 (;@15;)
                                  i32.const 0
                                  local.get 2
                                  i32.const -2
                                  local.get 3
                                  i32.rotl
                                  i32.and
                                  i32.store offset=1059044
                                  br 1 (;@14;)
                                end
                                i32.const 0
                                i32.load offset=1059060
                                local.get 6
                                i32.gt_u
                                drop
                                local.get 5
                                local.get 6
                                i32.store offset=8
                                local.get 6
                                local.get 5
                                i32.store offset=12
                              end
                              local.get 4
                              local.get 3
                              i32.const 3
                              i32.shl
                              local.tee 6
                              i32.const 3
                              i32.or
                              i32.store offset=4
                              local.get 4
                              local.get 6
                              i32.add
                              local.tee 4
                              local.get 4
                              i32.load offset=4
                              i32.const 1
                              i32.or
                              i32.store offset=4
                              br 12 (;@1;)
                            end
                            local.get 3
                            i32.const 0
                            i32.load offset=1059052
                            local.tee 7
                            i32.le_u
                            br_if 1 (;@11;)
                            block  ;; label = @13
                              local.get 0
                              i32.eqz
                              br_if 0 (;@13;)
                              block  ;; label = @14
                                block  ;; label = @15
                                  local.get 0
                                  local.get 4
                                  i32.shl
                                  i32.const 2
                                  local.get 4
                                  i32.shl
                                  local.tee 0
                                  i32.const 0
                                  local.get 0
                                  i32.sub
                                  i32.or
                                  i32.and
                                  local.tee 0
                                  i32.const 0
                                  local.get 0
                                  i32.sub
                                  i32.and
                                  i32.const -1
                                  i32.add
                                  local.tee 0
                                  local.get 0
                                  i32.const 12
                                  i32.shr_u
                                  i32.const 16
                                  i32.and
                                  local.tee 0
                                  i32.shr_u
                                  local.tee 4
                                  i32.const 5
                                  i32.shr_u
                                  i32.const 8
                                  i32.and
                                  local.tee 6
                                  local.get 0
                                  i32.or
                                  local.get 4
                                  local.get 6
                                  i32.shr_u
                                  local.tee 0
                                  i32.const 2
                                  i32.shr_u
                                  i32.const 4
                                  i32.and
                                  local.tee 4
                                  i32.or
                                  local.get 0
                                  local.get 4
                                  i32.shr_u
                                  local.tee 0
                                  i32.const 1
                                  i32.shr_u
                                  i32.const 2
                                  i32.and
                                  local.tee 4
                                  i32.or
                                  local.get 0
                                  local.get 4
                                  i32.shr_u
                                  local.tee 0
                                  i32.const 1
                                  i32.shr_u
                                  i32.const 1
                                  i32.and
                                  local.tee 4
                                  i32.or
                                  local.get 0
                                  local.get 4
                                  i32.shr_u
                                  i32.add
                                  local.tee 6
                                  i32.const 3
                                  i32.shl
                                  local.tee 5
                                  i32.const 1059092
                                  i32.add
                                  i32.load
                                  local.tee 4
                                  i32.load offset=8
                                  local.tee 0
                                  local.get 5
                                  i32.const 1059084
                                  i32.add
                                  local.tee 5
                                  i32.ne
                                  br_if 0 (;@15;)
                                  i32.const 0
                                  local.get 2
                                  i32.const -2
                                  local.get 6
                                  i32.rotl
                                  i32.and
                                  local.tee 2
                                  i32.store offset=1059044
                                  br 1 (;@14;)
                                end
                                i32.const 0
                                i32.load offset=1059060
                                local.get 0
                                i32.gt_u
                                drop
                                local.get 5
                                local.get 0
                                i32.store offset=8
                                local.get 0
                                local.get 5
                                i32.store offset=12
                              end
                              local.get 4
                              i32.const 8
                              i32.add
                              local.set 0
                              local.get 4
                              local.get 3
                              i32.const 3
                              i32.or
                              i32.store offset=4
                              local.get 4
                              local.get 6
                              i32.const 3
                              i32.shl
                              local.tee 6
                              i32.add
                              local.get 6
                              local.get 3
                              i32.sub
                              local.tee 6
                              i32.store
                              local.get 4
                              local.get 3
                              i32.add
                              local.tee 5
                              local.get 6
                              i32.const 1
                              i32.or
                              i32.store offset=4
                              block  ;; label = @14
                                local.get 7
                                i32.eqz
                                br_if 0 (;@14;)
                                local.get 7
                                i32.const 3
                                i32.shr_u
                                local.tee 8
                                i32.const 3
                                i32.shl
                                i32.const 1059084
                                i32.add
                                local.set 3
                                i32.const 0
                                i32.load offset=1059064
                                local.set 4
                                block  ;; label = @15
                                  block  ;; label = @16
                                    local.get 2
                                    i32.const 1
                                    local.get 8
                                    i32.shl
                                    local.tee 8
                                    i32.and
                                    br_if 0 (;@16;)
                                    i32.const 0
                                    local.get 2
                                    local.get 8
                                    i32.or
                                    i32.store offset=1059044
                                    local.get 3
                                    local.set 8
                                    br 1 (;@15;)
                                  end
                                  local.get 3
                                  i32.load offset=8
                                  local.set 8
                                end
                                local.get 8
                                local.get 4
                                i32.store offset=12
                                local.get 3
                                local.get 4
                                i32.store offset=8
                                local.get 4
                                local.get 3
                                i32.store offset=12
                                local.get 4
                                local.get 8
                                i32.store offset=8
                              end
                              i32.const 0
                              local.get 5
                              i32.store offset=1059064
                              i32.const 0
                              local.get 6
                              i32.store offset=1059052
                              br 12 (;@1;)
                            end
                            i32.const 0
                            i32.load offset=1059048
                            local.tee 9
                            i32.eqz
                            br_if 1 (;@11;)
                            local.get 9
                            i32.const 0
                            local.get 9
                            i32.sub
                            i32.and
                            i32.const -1
                            i32.add
                            local.tee 0
                            local.get 0
                            i32.const 12
                            i32.shr_u
                            i32.const 16
                            i32.and
                            local.tee 0
                            i32.shr_u
                            local.tee 4
                            i32.const 5
                            i32.shr_u
                            i32.const 8
                            i32.and
                            local.tee 6
                            local.get 0
                            i32.or
                            local.get 4
                            local.get 6
                            i32.shr_u
                            local.tee 0
                            i32.const 2
                            i32.shr_u
                            i32.const 4
                            i32.and
                            local.tee 4
                            i32.or
                            local.get 0
                            local.get 4
                            i32.shr_u
                            local.tee 0
                            i32.const 1
                            i32.shr_u
                            i32.const 2
                            i32.and
                            local.tee 4
                            i32.or
                            local.get 0
                            local.get 4
                            i32.shr_u
                            local.tee 0
                            i32.const 1
                            i32.shr_u
                            i32.const 1
                            i32.and
                            local.tee 4
                            i32.or
                            local.get 0
                            local.get 4
                            i32.shr_u
                            i32.add
                            i32.const 2
                            i32.shl
                            i32.const 1059348
                            i32.add
                            i32.load
                            local.tee 5
                            i32.load offset=4
                            i32.const -8
                            i32.and
                            local.get 3
                            i32.sub
                            local.set 4
                            local.get 5
                            local.set 6
                            block  ;; label = @13
                              loop  ;; label = @14
                                block  ;; label = @15
                                  local.get 6
                                  i32.load offset=16
                                  local.tee 0
                                  br_if 0 (;@15;)
                                  local.get 6
                                  i32.const 20
                                  i32.add
                                  i32.load
                                  local.tee 0
                                  i32.eqz
                                  br_if 2 (;@13;)
                                end
                                local.get 0
                                i32.load offset=4
                                i32.const -8
                                i32.and
                                local.get 3
                                i32.sub
                                local.tee 6
                                local.get 4
                                local.get 6
                                local.get 4
                                i32.lt_u
                                local.tee 6
                                select
                                local.set 4
                                local.get 0
                                local.get 5
                                local.get 6
                                select
                                local.set 5
                                local.get 0
                                local.set 6
                                br 0 (;@14;)
                              end
                            end
                            local.get 5
                            i32.load offset=24
                            local.set 10
                            block  ;; label = @13
                              local.get 5
                              i32.load offset=12
                              local.tee 8
                              local.get 5
                              i32.eq
                              br_if 0 (;@13;)
                              block  ;; label = @14
                                i32.const 0
                                i32.load offset=1059060
                                local.get 5
                                i32.load offset=8
                                local.tee 0
                                i32.gt_u
                                br_if 0 (;@14;)
                                local.get 0
                                i32.load offset=12
                                local.get 5
                                i32.ne
                                drop
                              end
                              local.get 8
                              local.get 0
                              i32.store offset=8
                              local.get 0
                              local.get 8
                              i32.store offset=12
                              br 11 (;@2;)
                            end
                            block  ;; label = @13
                              local.get 5
                              i32.const 20
                              i32.add
                              local.tee 6
                              i32.load
                              local.tee 0
                              br_if 0 (;@13;)
                              local.get 5
                              i32.load offset=16
                              local.tee 0
                              i32.eqz
                              br_if 3 (;@10;)
                              local.get 5
                              i32.const 16
                              i32.add
                              local.set 6
                            end
                            loop  ;; label = @13
                              local.get 6
                              local.set 11
                              local.get 0
                              local.tee 8
                              i32.const 20
                              i32.add
                              local.tee 6
                              i32.load
                              local.tee 0
                              br_if 0 (;@13;)
                              local.get 8
                              i32.const 16
                              i32.add
                              local.set 6
                              local.get 8
                              i32.load offset=16
                              local.tee 0
                              br_if 0 (;@13;)
                            end
                            local.get 11
                            i32.const 0
                            i32.store
                            br 10 (;@2;)
                          end
                          i32.const -1
                          local.set 3
                          local.get 0
                          i32.const -65
                          i32.gt_u
                          br_if 0 (;@11;)
                          local.get 0
                          i32.const 19
                          i32.add
                          local.tee 0
                          i32.const -16
                          i32.and
                          local.set 3
                          i32.const 0
                          i32.load offset=1059048
                          local.tee 7
                          i32.eqz
                          br_if 0 (;@11;)
                          i32.const 0
                          local.set 11
                          block  ;; label = @12
                            local.get 0
                            i32.const 8
                            i32.shr_u
                            local.tee 0
                            i32.eqz
                            br_if 0 (;@12;)
                            i32.const 31
                            local.set 11
                            local.get 3
                            i32.const 16777215
                            i32.gt_u
                            br_if 0 (;@12;)
                            local.get 0
                            local.get 0
                            i32.const 1048320
                            i32.add
                            i32.const 16
                            i32.shr_u
                            i32.const 8
                            i32.and
                            local.tee 4
                            i32.shl
                            local.tee 0
                            local.get 0
                            i32.const 520192
                            i32.add
                            i32.const 16
                            i32.shr_u
                            i32.const 4
                            i32.and
                            local.tee 0
                            i32.shl
                            local.tee 6
                            local.get 6
                            i32.const 245760
                            i32.add
                            i32.const 16
                            i32.shr_u
                            i32.const 2
                            i32.and
                            local.tee 6
                            i32.shl
                            i32.const 15
                            i32.shr_u
                            local.get 0
                            local.get 4
                            i32.or
                            local.get 6
                            i32.or
                            i32.sub
                            local.tee 0
                            i32.const 1
                            i32.shl
                            local.get 3
                            local.get 0
                            i32.const 21
                            i32.add
                            i32.shr_u
                            i32.const 1
                            i32.and
                            i32.or
                            i32.const 28
                            i32.add
                            local.set 11
                          end
                          i32.const 0
                          local.get 3
                          i32.sub
                          local.set 6
                          block  ;; label = @12
                            block  ;; label = @13
                              block  ;; label = @14
                                block  ;; label = @15
                                  local.get 11
                                  i32.const 2
                                  i32.shl
                                  i32.const 1059348
                                  i32.add
                                  i32.load
                                  local.tee 4
                                  br_if 0 (;@15;)
                                  i32.const 0
                                  local.set 0
                                  i32.const 0
                                  local.set 8
                                  br 1 (;@14;)
                                end
                                local.get 3
                                i32.const 0
                                i32.const 25
                                local.get 11
                                i32.const 1
                                i32.shr_u
                                i32.sub
                                local.get 11
                                i32.const 31
                                i32.eq
                                select
                                i32.shl
                                local.set 5
                                i32.const 0
                                local.set 0
                                i32.const 0
                                local.set 8
                                loop  ;; label = @15
                                  block  ;; label = @16
                                    local.get 4
                                    i32.load offset=4
                                    i32.const -8
                                    i32.and
                                    local.get 3
                                    i32.sub
                                    local.tee 2
                                    local.get 6
                                    i32.ge_u
                                    br_if 0 (;@16;)
                                    local.get 2
                                    local.set 6
                                    local.get 4
                                    local.set 8
                                    local.get 2
                                    br_if 0 (;@16;)
                                    i32.const 0
                                    local.set 6
                                    local.get 4
                                    local.set 8
                                    local.get 4
                                    local.set 0
                                    br 3 (;@13;)
                                  end
                                  local.get 0
                                  local.get 4
                                  i32.const 20
                                  i32.add
                                  i32.load
                                  local.tee 2
                                  local.get 2
                                  local.get 4
                                  local.get 5
                                  i32.const 29
                                  i32.shr_u
                                  i32.const 4
                                  i32.and
                                  i32.add
                                  i32.const 16
                                  i32.add
                                  i32.load
                                  local.tee 4
                                  i32.eq
                                  select
                                  local.get 0
                                  local.get 2
                                  select
                                  local.set 0
                                  local.get 5
                                  local.get 4
                                  i32.const 0
                                  i32.ne
                                  i32.shl
                                  local.set 5
                                  local.get 4
                                  br_if 0 (;@15;)
                                end
                              end
                              block  ;; label = @14
                                local.get 0
                                local.get 8
                                i32.or
                                br_if 0 (;@14;)
                                i32.const 2
                                local.get 11
                                i32.shl
                                local.tee 0
                                i32.const 0
                                local.get 0
                                i32.sub
                                i32.or
                                local.get 7
                                i32.and
                                local.tee 0
                                i32.eqz
                                br_if 3 (;@11;)
                                local.get 0
                                i32.const 0
                                local.get 0
                                i32.sub
                                i32.and
                                i32.const -1
                                i32.add
                                local.tee 0
                                local.get 0
                                i32.const 12
                                i32.shr_u
                                i32.const 16
                                i32.and
                                local.tee 0
                                i32.shr_u
                                local.tee 4
                                i32.const 5
                                i32.shr_u
                                i32.const 8
                                i32.and
                                local.tee 5
                                local.get 0
                                i32.or
                                local.get 4
                                local.get 5
                                i32.shr_u
                                local.tee 0
                                i32.const 2
                                i32.shr_u
                                i32.const 4
                                i32.and
                                local.tee 4
                                i32.or
                                local.get 0
                                local.get 4
                                i32.shr_u
                                local.tee 0
                                i32.const 1
                                i32.shr_u
                                i32.const 2
                                i32.and
                                local.tee 4
                                i32.or
                                local.get 0
                                local.get 4
                                i32.shr_u
                                local.tee 0
                                i32.const 1
                                i32.shr_u
                                i32.const 1
                                i32.and
                                local.tee 4
                                i32.or
                                local.get 0
                                local.get 4
                                i32.shr_u
                                i32.add
                                i32.const 2
                                i32.shl
                                i32.const 1059348
                                i32.add
                                i32.load
                                local.set 0
                              end
                              local.get 0
                              i32.eqz
                              br_if 1 (;@12;)
                            end
                            loop  ;; label = @13
                              local.get 0
                              i32.load offset=4
                              i32.const -8
                              i32.and
                              local.get 3
                              i32.sub
                              local.tee 2
                              local.get 6
                              i32.lt_u
                              local.set 5
                              block  ;; label = @14
                                local.get 0
                                i32.load offset=16
                                local.tee 4
                                br_if 0 (;@14;)
                                local.get 0
                                i32.const 20
                                i32.add
                                i32.load
                                local.set 4
                              end
                              local.get 2
                              local.get 6
                              local.get 5
                              select
                              local.set 6
                              local.get 0
                              local.get 8
                              local.get 5
                              select
                              local.set 8
                              local.get 4
                              local.set 0
                              local.get 4
                              br_if 0 (;@13;)
                            end
                          end
                          local.get 8
                          i32.eqz
                          br_if 0 (;@11;)
                          local.get 6
                          i32.const 0
                          i32.load offset=1059052
                          local.get 3
                          i32.sub
                          i32.ge_u
                          br_if 0 (;@11;)
                          local.get 8
                          i32.load offset=24
                          local.set 11
                          block  ;; label = @12
                            local.get 8
                            i32.load offset=12
                            local.tee 5
                            local.get 8
                            i32.eq
                            br_if 0 (;@12;)
                            block  ;; label = @13
                              i32.const 0
                              i32.load offset=1059060
                              local.get 8
                              i32.load offset=8
                              local.tee 0
                              i32.gt_u
                              br_if 0 (;@13;)
                              local.get 0
                              i32.load offset=12
                              local.get 8
                              i32.ne
                              drop
                            end
                            local.get 5
                            local.get 0
                            i32.store offset=8
                            local.get 0
                            local.get 5
                            i32.store offset=12
                            br 9 (;@3;)
                          end
                          block  ;; label = @12
                            local.get 8
                            i32.const 20
                            i32.add
                            local.tee 4
                            i32.load
                            local.tee 0
                            br_if 0 (;@12;)
                            local.get 8
                            i32.load offset=16
                            local.tee 0
                            i32.eqz
                            br_if 3 (;@9;)
                            local.get 8
                            i32.const 16
                            i32.add
                            local.set 4
                          end
                          loop  ;; label = @12
                            local.get 4
                            local.set 2
                            local.get 0
                            local.tee 5
                            i32.const 20
                            i32.add
                            local.tee 4
                            i32.load
                            local.tee 0
                            br_if 0 (;@12;)
                            local.get 5
                            i32.const 16
                            i32.add
                            local.set 4
                            local.get 5
                            i32.load offset=16
                            local.tee 0
                            br_if 0 (;@12;)
                          end
                          local.get 2
                          i32.const 0
                          i32.store
                          br 8 (;@3;)
                        end
                        block  ;; label = @11
                          i32.const 0
                          i32.load offset=1059052
                          local.tee 0
                          local.get 3
                          i32.lt_u
                          br_if 0 (;@11;)
                          i32.const 0
                          i32.load offset=1059064
                          local.set 4
                          block  ;; label = @12
                            block  ;; label = @13
                              local.get 0
                              local.get 3
                              i32.sub
                              local.tee 6
                              i32.const 16
                              i32.lt_u
                              br_if 0 (;@13;)
                              local.get 4
                              local.get 3
                              i32.add
                              local.tee 5
                              local.get 6
                              i32.const 1
                              i32.or
                              i32.store offset=4
                              i32.const 0
                              local.get 6
                              i32.store offset=1059052
                              i32.const 0
                              local.get 5
                              i32.store offset=1059064
                              local.get 4
                              local.get 0
                              i32.add
                              local.get 6
                              i32.store
                              local.get 4
                              local.get 3
                              i32.const 3
                              i32.or
                              i32.store offset=4
                              br 1 (;@12;)
                            end
                            local.get 4
                            local.get 0
                            i32.const 3
                            i32.or
                            i32.store offset=4
                            local.get 4
                            local.get 0
                            i32.add
                            local.tee 0
                            local.get 0
                            i32.load offset=4
                            i32.const 1
                            i32.or
                            i32.store offset=4
                            i32.const 0
                            i32.const 0
                            i32.store offset=1059064
                            i32.const 0
                            i32.const 0
                            i32.store offset=1059052
                          end
                          local.get 4
                          i32.const 8
                          i32.add
                          local.set 0
                          br 10 (;@1;)
                        end
                        block  ;; label = @11
                          i32.const 0
                          i32.load offset=1059056
                          local.tee 5
                          local.get 3
                          i32.le_u
                          br_if 0 (;@11;)
                          i32.const 0
                          i32.load offset=1059068
                          local.tee 0
                          local.get 3
                          i32.add
                          local.tee 4
                          local.get 5
                          local.get 3
                          i32.sub
                          local.tee 6
                          i32.const 1
                          i32.or
                          i32.store offset=4
                          i32.const 0
                          local.get 6
                          i32.store offset=1059056
                          i32.const 0
                          local.get 4
                          i32.store offset=1059068
                          local.get 0
                          local.get 3
                          i32.const 3
                          i32.or
                          i32.store offset=4
                          local.get 0
                          i32.const 8
                          i32.add
                          local.set 0
                          br 10 (;@1;)
                        end
                        block  ;; label = @11
                          block  ;; label = @12
                            i32.const 0
                            i32.load offset=1059516
                            i32.eqz
                            br_if 0 (;@12;)
                            i32.const 0
                            i32.load offset=1059524
                            local.set 4
                            br 1 (;@11;)
                          end
                          i32.const 0
                          i64.const -1
                          i64.store offset=1059528 align=4
                          i32.const 0
                          i64.const 281474976776192
                          i64.store offset=1059520 align=4
                          i32.const 0
                          local.get 1
                          i32.const 12
                          i32.add
                          i32.const -16
                          i32.and
                          i32.const 1431655768
                          i32.xor
                          i32.store offset=1059516
                          i32.const 0
                          i32.const 0
                          i32.store offset=1059536
                          i32.const 0
                          i32.const 0
                          i32.store offset=1059488
                          i32.const 65536
                          local.set 4
                        end
                        i32.const 0
                        local.set 0
                        block  ;; label = @11
                          local.get 4
                          local.get 3
                          i32.const 71
                          i32.add
                          local.tee 7
                          i32.add
                          local.tee 2
                          i32.const 0
                          local.get 4
                          i32.sub
                          local.tee 11
                          i32.and
                          local.tee 8
                          local.get 3
                          i32.gt_u
                          br_if 0 (;@11;)
                          i32.const 0
                          i32.const 48
                          i32.store offset=1059540
                          br 10 (;@1;)
                        end
                        block  ;; label = @11
                          i32.const 0
                          i32.load offset=1059484
                          local.tee 0
                          i32.eqz
                          br_if 0 (;@11;)
                          block  ;; label = @12
                            i32.const 0
                            i32.load offset=1059476
                            local.tee 4
                            local.get 8
                            i32.add
                            local.tee 6
                            local.get 4
                            i32.le_u
                            br_if 0 (;@12;)
                            local.get 6
                            local.get 0
                            i32.le_u
                            br_if 1 (;@11;)
                          end
                          i32.const 0
                          local.set 0
                          i32.const 0
                          i32.const 48
                          i32.store offset=1059540
                          br 10 (;@1;)
                        end
                        i32.const 0
                        i32.load8_u offset=1059488
                        i32.const 4
                        i32.and
                        br_if 4 (;@6;)
                        block  ;; label = @11
                          block  ;; label = @12
                            block  ;; label = @13
                              i32.const 0
                              i32.load offset=1059068
                              local.tee 4
                              i32.eqz
                              br_if 0 (;@13;)
                              i32.const 1059492
                              local.set 0
                              loop  ;; label = @14
                                block  ;; label = @15
                                  local.get 0
                                  i32.load
                                  local.tee 6
                                  local.get 4
                                  i32.gt_u
                                  br_if 0 (;@15;)
                                  local.get 6
                                  local.get 0
                                  i32.load offset=4
                                  i32.add
                                  local.get 4
                                  i32.gt_u
                                  br_if 3 (;@12;)
                                end
                                local.get 0
                                i32.load offset=8
                                local.tee 0
                                br_if 0 (;@14;)
                              end
                            end
                            i32.const 0
                            call $sbrk
                            local.tee 5
                            i32.const -1
                            i32.eq
                            br_if 5 (;@7;)
                            local.get 8
                            local.set 2
                            block  ;; label = @13
                              i32.const 0
                              i32.load offset=1059520
                              local.tee 0
                              i32.const -1
                              i32.add
                              local.tee 4
                              local.get 5
                              i32.and
                              i32.eqz
                              br_if 0 (;@13;)
                              local.get 8
                              local.get 5
                              i32.sub
                              local.get 4
                              local.get 5
                              i32.add
                              i32.const 0
                              local.get 0
                              i32.sub
                              i32.and
                              i32.add
                              local.set 2
                            end
                            local.get 2
                            local.get 3
                            i32.le_u
                            br_if 5 (;@7;)
                            local.get 2
                            i32.const 2147483646
                            i32.gt_u
                            br_if 5 (;@7;)
                            block  ;; label = @13
                              i32.const 0
                              i32.load offset=1059484
                              local.tee 0
                              i32.eqz
                              br_if 0 (;@13;)
                              i32.const 0
                              i32.load offset=1059476
                              local.tee 4
                              local.get 2
                              i32.add
                              local.tee 6
                              local.get 4
                              i32.le_u
                              br_if 6 (;@7;)
                              local.get 6
                              local.get 0
                              i32.gt_u
                              br_if 6 (;@7;)
                            end
                            local.get 2
                            call $sbrk
                            local.tee 0
                            local.get 5
                            i32.ne
                            br_if 1 (;@11;)
                            br 7 (;@5;)
                          end
                          local.get 2
                          local.get 5
                          i32.sub
                          local.get 11
                          i32.and
                          local.tee 2
                          i32.const 2147483646
                          i32.gt_u
                          br_if 4 (;@7;)
                          local.get 2
                          call $sbrk
                          local.tee 5
                          local.get 0
                          i32.load
                          local.get 0
                          i32.load offset=4
                          i32.add
                          i32.eq
                          br_if 3 (;@8;)
                          local.get 5
                          local.set 0
                        end
                        local.get 0
                        local.set 5
                        block  ;; label = @11
                          local.get 3
                          i32.const 72
                          i32.add
                          local.get 2
                          i32.le_u
                          br_if 0 (;@11;)
                          local.get 2
                          i32.const 2147483646
                          i32.gt_u
                          br_if 0 (;@11;)
                          local.get 5
                          i32.const -1
                          i32.eq
                          br_if 0 (;@11;)
                          local.get 7
                          local.get 2
                          i32.sub
                          i32.const 0
                          i32.load offset=1059524
                          local.tee 0
                          i32.add
                          i32.const 0
                          local.get 0
                          i32.sub
                          i32.and
                          local.tee 0
                          i32.const 2147483646
                          i32.gt_u
                          br_if 6 (;@5;)
                          block  ;; label = @12
                            local.get 0
                            call $sbrk
                            i32.const -1
                            i32.eq
                            br_if 0 (;@12;)
                            local.get 0
                            local.get 2
                            i32.add
                            local.set 2
                            br 7 (;@5;)
                          end
                          i32.const 0
                          local.get 2
                          i32.sub
                          call $sbrk
                          drop
                          br 4 (;@7;)
                        end
                        local.get 5
                        i32.const -1
                        i32.ne
                        br_if 5 (;@5;)
                        br 3 (;@7;)
                      end
                      i32.const 0
                      local.set 8
                      br 7 (;@2;)
                    end
                    i32.const 0
                    local.set 5
                    br 5 (;@3;)
                  end
                  local.get 5
                  i32.const -1
                  i32.ne
                  br_if 2 (;@5;)
                end
                i32.const 0
                i32.const 0
                i32.load offset=1059488
                i32.const 4
                i32.or
                i32.store offset=1059488
              end
              local.get 8
              i32.const 2147483646
              i32.gt_u
              br_if 1 (;@4;)
              local.get 8
              call $sbrk
              local.tee 5
              i32.const 0
              call $sbrk
              local.tee 0
              i32.ge_u
              br_if 1 (;@4;)
              local.get 5
              i32.const -1
              i32.eq
              br_if 1 (;@4;)
              local.get 0
              i32.const -1
              i32.eq
              br_if 1 (;@4;)
              local.get 0
              local.get 5
              i32.sub
              local.tee 2
              local.get 3
              i32.const 56
              i32.add
              i32.le_u
              br_if 1 (;@4;)
            end
            i32.const 0
            i32.const 0
            i32.load offset=1059476
            local.get 2
            i32.add
            local.tee 0
            i32.store offset=1059476
            block  ;; label = @5
              local.get 0
              i32.const 0
              i32.load offset=1059480
              i32.le_u
              br_if 0 (;@5;)
              i32.const 0
              local.get 0
              i32.store offset=1059480
            end
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    i32.const 0
                    i32.load offset=1059068
                    local.tee 4
                    i32.eqz
                    br_if 0 (;@8;)
                    i32.const 1059492
                    local.set 0
                    loop  ;; label = @9
                      local.get 5
                      local.get 0
                      i32.load
                      local.tee 6
                      local.get 0
                      i32.load offset=4
                      local.tee 8
                      i32.add
                      i32.eq
                      br_if 2 (;@7;)
                      local.get 0
                      i32.load offset=8
                      local.tee 0
                      br_if 0 (;@9;)
                      br 3 (;@6;)
                    end
                  end
                  block  ;; label = @8
                    block  ;; label = @9
                      i32.const 0
                      i32.load offset=1059060
                      local.tee 0
                      i32.eqz
                      br_if 0 (;@9;)
                      local.get 5
                      local.get 0
                      i32.ge_u
                      br_if 1 (;@8;)
                    end
                    i32.const 0
                    local.get 5
                    i32.store offset=1059060
                  end
                  i32.const 0
                  local.set 0
                  i32.const 0
                  local.get 2
                  i32.store offset=1059496
                  i32.const 0
                  local.get 5
                  i32.store offset=1059492
                  i32.const 0
                  i32.const -1
                  i32.store offset=1059076
                  i32.const 0
                  i32.const 0
                  i32.load offset=1059516
                  i32.store offset=1059080
                  i32.const 0
                  i32.const 0
                  i32.store offset=1059504
                  loop  ;; label = @8
                    local.get 0
                    i32.const 1059092
                    i32.add
                    local.get 0
                    i32.const 1059084
                    i32.add
                    local.tee 4
                    i32.store
                    local.get 0
                    i32.const 1059096
                    i32.add
                    local.get 4
                    i32.store
                    local.get 0
                    i32.const 8
                    i32.add
                    local.tee 0
                    i32.const 256
                    i32.ne
                    br_if 0 (;@8;)
                  end
                  local.get 5
                  i32.const -8
                  local.get 5
                  i32.sub
                  i32.const 15
                  i32.and
                  i32.const 0
                  local.get 5
                  i32.const 8
                  i32.add
                  i32.const 15
                  i32.and
                  select
                  local.tee 0
                  i32.add
                  local.tee 4
                  local.get 2
                  i32.const -56
                  i32.add
                  local.tee 6
                  local.get 0
                  i32.sub
                  local.tee 0
                  i32.const 1
                  i32.or
                  i32.store offset=4
                  i32.const 0
                  i32.const 0
                  i32.load offset=1059532
                  i32.store offset=1059072
                  i32.const 0
                  local.get 0
                  i32.store offset=1059056
                  i32.const 0
                  local.get 4
                  i32.store offset=1059068
                  local.get 5
                  local.get 6
                  i32.add
                  i32.const 56
                  i32.store offset=4
                  br 2 (;@5;)
                end
                local.get 0
                i32.load8_u offset=12
                i32.const 8
                i32.and
                br_if 0 (;@6;)
                local.get 5
                local.get 4
                i32.le_u
                br_if 0 (;@6;)
                local.get 6
                local.get 4
                i32.gt_u
                br_if 0 (;@6;)
                local.get 4
                i32.const -8
                local.get 4
                i32.sub
                i32.const 15
                i32.and
                i32.const 0
                local.get 4
                i32.const 8
                i32.add
                i32.const 15
                i32.and
                select
                local.tee 6
                i32.add
                local.tee 5
                i32.const 0
                i32.load offset=1059056
                local.get 2
                i32.add
                local.tee 11
                local.get 6
                i32.sub
                local.tee 6
                i32.const 1
                i32.or
                i32.store offset=4
                local.get 0
                local.get 8
                local.get 2
                i32.add
                i32.store offset=4
                i32.const 0
                i32.const 0
                i32.load offset=1059532
                i32.store offset=1059072
                i32.const 0
                local.get 6
                i32.store offset=1059056
                i32.const 0
                local.get 5
                i32.store offset=1059068
                local.get 4
                local.get 11
                i32.add
                i32.const 56
                i32.store offset=4
                br 1 (;@5;)
              end
              block  ;; label = @6
                local.get 5
                i32.const 0
                i32.load offset=1059060
                local.tee 8
                i32.ge_u
                br_if 0 (;@6;)
                i32.const 0
                local.get 5
                i32.store offset=1059060
                local.get 5
                local.set 8
              end
              local.get 5
              local.get 2
              i32.add
              local.set 6
              i32.const 1059492
              local.set 0
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      block  ;; label = @10
                        block  ;; label = @11
                          block  ;; label = @12
                            loop  ;; label = @13
                              local.get 0
                              i32.load
                              local.get 6
                              i32.eq
                              br_if 1 (;@12;)
                              local.get 0
                              i32.load offset=8
                              local.tee 0
                              br_if 0 (;@13;)
                              br 2 (;@11;)
                            end
                          end
                          local.get 0
                          i32.load8_u offset=12
                          i32.const 8
                          i32.and
                          i32.eqz
                          br_if 1 (;@10;)
                        end
                        i32.const 1059492
                        local.set 0
                        loop  ;; label = @11
                          block  ;; label = @12
                            local.get 0
                            i32.load
                            local.tee 6
                            local.get 4
                            i32.gt_u
                            br_if 0 (;@12;)
                            local.get 6
                            local.get 0
                            i32.load offset=4
                            i32.add
                            local.tee 6
                            local.get 4
                            i32.gt_u
                            br_if 3 (;@9;)
                          end
                          local.get 0
                          i32.load offset=8
                          local.set 0
                          br 0 (;@11;)
                        end
                      end
                      local.get 0
                      local.get 5
                      i32.store
                      local.get 0
                      local.get 0
                      i32.load offset=4
                      local.get 2
                      i32.add
                      i32.store offset=4
                      local.get 5
                      i32.const -8
                      local.get 5
                      i32.sub
                      i32.const 15
                      i32.and
                      i32.const 0
                      local.get 5
                      i32.const 8
                      i32.add
                      i32.const 15
                      i32.and
                      select
                      i32.add
                      local.tee 11
                      local.get 3
                      i32.const 3
                      i32.or
                      i32.store offset=4
                      local.get 6
                      i32.const -8
                      local.get 6
                      i32.sub
                      i32.const 15
                      i32.and
                      i32.const 0
                      local.get 6
                      i32.const 8
                      i32.add
                      i32.const 15
                      i32.and
                      select
                      i32.add
                      local.tee 5
                      local.get 11
                      i32.sub
                      local.get 3
                      i32.sub
                      local.set 0
                      local.get 11
                      local.get 3
                      i32.add
                      local.set 6
                      block  ;; label = @10
                        local.get 4
                        local.get 5
                        i32.ne
                        br_if 0 (;@10;)
                        i32.const 0
                        local.get 6
                        i32.store offset=1059068
                        i32.const 0
                        i32.const 0
                        i32.load offset=1059056
                        local.get 0
                        i32.add
                        local.tee 0
                        i32.store offset=1059056
                        local.get 6
                        local.get 0
                        i32.const 1
                        i32.or
                        i32.store offset=4
                        br 3 (;@7;)
                      end
                      block  ;; label = @10
                        i32.const 0
                        i32.load offset=1059064
                        local.get 5
                        i32.ne
                        br_if 0 (;@10;)
                        i32.const 0
                        local.get 6
                        i32.store offset=1059064
                        i32.const 0
                        i32.const 0
                        i32.load offset=1059052
                        local.get 0
                        i32.add
                        local.tee 0
                        i32.store offset=1059052
                        local.get 6
                        local.get 0
                        i32.const 1
                        i32.or
                        i32.store offset=4
                        local.get 6
                        local.get 0
                        i32.add
                        local.get 0
                        i32.store
                        br 3 (;@7;)
                      end
                      block  ;; label = @10
                        local.get 5
                        i32.load offset=4
                        local.tee 4
                        i32.const 3
                        i32.and
                        i32.const 1
                        i32.ne
                        br_if 0 (;@10;)
                        local.get 4
                        i32.const -8
                        i32.and
                        local.set 7
                        block  ;; label = @11
                          block  ;; label = @12
                            local.get 4
                            i32.const 255
                            i32.gt_u
                            br_if 0 (;@12;)
                            local.get 5
                            i32.load offset=12
                            local.set 3
                            block  ;; label = @13
                              local.get 5
                              i32.load offset=8
                              local.tee 2
                              local.get 4
                              i32.const 3
                              i32.shr_u
                              local.tee 9
                              i32.const 3
                              i32.shl
                              i32.const 1059084
                              i32.add
                              local.tee 4
                              i32.eq
                              br_if 0 (;@13;)
                              local.get 8
                              local.get 2
                              i32.gt_u
                              drop
                            end
                            block  ;; label = @13
                              local.get 3
                              local.get 2
                              i32.ne
                              br_if 0 (;@13;)
                              i32.const 0
                              i32.const 0
                              i32.load offset=1059044
                              i32.const -2
                              local.get 9
                              i32.rotl
                              i32.and
                              i32.store offset=1059044
                              br 2 (;@11;)
                            end
                            block  ;; label = @13
                              local.get 3
                              local.get 4
                              i32.eq
                              br_if 0 (;@13;)
                              local.get 8
                              local.get 3
                              i32.gt_u
                              drop
                            end
                            local.get 3
                            local.get 2
                            i32.store offset=8
                            local.get 2
                            local.get 3
                            i32.store offset=12
                            br 1 (;@11;)
                          end
                          local.get 5
                          i32.load offset=24
                          local.set 9
                          block  ;; label = @12
                            block  ;; label = @13
                              local.get 5
                              i32.load offset=12
                              local.tee 2
                              local.get 5
                              i32.eq
                              br_if 0 (;@13;)
                              block  ;; label = @14
                                local.get 8
                                local.get 5
                                i32.load offset=8
                                local.tee 4
                                i32.gt_u
                                br_if 0 (;@14;)
                                local.get 4
                                i32.load offset=12
                                local.get 5
                                i32.ne
                                drop
                              end
                              local.get 2
                              local.get 4
                              i32.store offset=8
                              local.get 4
                              local.get 2
                              i32.store offset=12
                              br 1 (;@12;)
                            end
                            block  ;; label = @13
                              local.get 5
                              i32.const 20
                              i32.add
                              local.tee 4
                              i32.load
                              local.tee 3
                              br_if 0 (;@13;)
                              local.get 5
                              i32.const 16
                              i32.add
                              local.tee 4
                              i32.load
                              local.tee 3
                              br_if 0 (;@13;)
                              i32.const 0
                              local.set 2
                              br 1 (;@12;)
                            end
                            loop  ;; label = @13
                              local.get 4
                              local.set 8
                              local.get 3
                              local.tee 2
                              i32.const 20
                              i32.add
                              local.tee 4
                              i32.load
                              local.tee 3
                              br_if 0 (;@13;)
                              local.get 2
                              i32.const 16
                              i32.add
                              local.set 4
                              local.get 2
                              i32.load offset=16
                              local.tee 3
                              br_if 0 (;@13;)
                            end
                            local.get 8
                            i32.const 0
                            i32.store
                          end
                          local.get 9
                          i32.eqz
                          br_if 0 (;@11;)
                          block  ;; label = @12
                            block  ;; label = @13
                              local.get 5
                              i32.load offset=28
                              local.tee 3
                              i32.const 2
                              i32.shl
                              i32.const 1059348
                              i32.add
                              local.tee 4
                              i32.load
                              local.get 5
                              i32.ne
                              br_if 0 (;@13;)
                              local.get 4
                              local.get 2
                              i32.store
                              local.get 2
                              br_if 1 (;@12;)
                              i32.const 0
                              i32.const 0
                              i32.load offset=1059048
                              i32.const -2
                              local.get 3
                              i32.rotl
                              i32.and
                              i32.store offset=1059048
                              br 2 (;@11;)
                            end
                            local.get 9
                            i32.const 16
                            i32.const 20
                            local.get 9
                            i32.load offset=16
                            local.get 5
                            i32.eq
                            select
                            i32.add
                            local.get 2
                            i32.store
                            local.get 2
                            i32.eqz
                            br_if 1 (;@11;)
                          end
                          local.get 2
                          local.get 9
                          i32.store offset=24
                          block  ;; label = @12
                            local.get 5
                            i32.load offset=16
                            local.tee 4
                            i32.eqz
                            br_if 0 (;@12;)
                            local.get 2
                            local.get 4
                            i32.store offset=16
                            local.get 4
                            local.get 2
                            i32.store offset=24
                          end
                          local.get 5
                          i32.load offset=20
                          local.tee 4
                          i32.eqz
                          br_if 0 (;@11;)
                          local.get 2
                          i32.const 20
                          i32.add
                          local.get 4
                          i32.store
                          local.get 4
                          local.get 2
                          i32.store offset=24
                        end
                        local.get 7
                        local.get 0
                        i32.add
                        local.set 0
                        local.get 5
                        local.get 7
                        i32.add
                        local.set 5
                      end
                      local.get 5
                      local.get 5
                      i32.load offset=4
                      i32.const -2
                      i32.and
                      i32.store offset=4
                      local.get 6
                      local.get 0
                      i32.add
                      local.get 0
                      i32.store
                      local.get 6
                      local.get 0
                      i32.const 1
                      i32.or
                      i32.store offset=4
                      block  ;; label = @10
                        local.get 0
                        i32.const 255
                        i32.gt_u
                        br_if 0 (;@10;)
                        local.get 0
                        i32.const 3
                        i32.shr_u
                        local.tee 4
                        i32.const 3
                        i32.shl
                        i32.const 1059084
                        i32.add
                        local.set 0
                        block  ;; label = @11
                          block  ;; label = @12
                            i32.const 0
                            i32.load offset=1059044
                            local.tee 3
                            i32.const 1
                            local.get 4
                            i32.shl
                            local.tee 4
                            i32.and
                            br_if 0 (;@12;)
                            i32.const 0
                            local.get 3
                            local.get 4
                            i32.or
                            i32.store offset=1059044
                            local.get 0
                            local.set 4
                            br 1 (;@11;)
                          end
                          local.get 0
                          i32.load offset=8
                          local.set 4
                        end
                        local.get 4
                        local.get 6
                        i32.store offset=12
                        local.get 0
                        local.get 6
                        i32.store offset=8
                        local.get 6
                        local.get 0
                        i32.store offset=12
                        local.get 6
                        local.get 4
                        i32.store offset=8
                        br 3 (;@7;)
                      end
                      i32.const 0
                      local.set 4
                      block  ;; label = @10
                        local.get 0
                        i32.const 8
                        i32.shr_u
                        local.tee 3
                        i32.eqz
                        br_if 0 (;@10;)
                        i32.const 31
                        local.set 4
                        local.get 0
                        i32.const 16777215
                        i32.gt_u
                        br_if 0 (;@10;)
                        local.get 3
                        local.get 3
                        i32.const 1048320
                        i32.add
                        i32.const 16
                        i32.shr_u
                        i32.const 8
                        i32.and
                        local.tee 4
                        i32.shl
                        local.tee 3
                        local.get 3
                        i32.const 520192
                        i32.add
                        i32.const 16
                        i32.shr_u
                        i32.const 4
                        i32.and
                        local.tee 3
                        i32.shl
                        local.tee 5
                        local.get 5
                        i32.const 245760
                        i32.add
                        i32.const 16
                        i32.shr_u
                        i32.const 2
                        i32.and
                        local.tee 5
                        i32.shl
                        i32.const 15
                        i32.shr_u
                        local.get 3
                        local.get 4
                        i32.or
                        local.get 5
                        i32.or
                        i32.sub
                        local.tee 4
                        i32.const 1
                        i32.shl
                        local.get 0
                        local.get 4
                        i32.const 21
                        i32.add
                        i32.shr_u
                        i32.const 1
                        i32.and
                        i32.or
                        i32.const 28
                        i32.add
                        local.set 4
                      end
                      local.get 6
                      local.get 4
                      i32.store offset=28
                      local.get 6
                      i64.const 0
                      i64.store offset=16 align=4
                      local.get 4
                      i32.const 2
                      i32.shl
                      i32.const 1059348
                      i32.add
                      local.set 3
                      block  ;; label = @10
                        i32.const 0
                        i32.load offset=1059048
                        local.tee 5
                        i32.const 1
                        local.get 4
                        i32.shl
                        local.tee 8
                        i32.and
                        br_if 0 (;@10;)
                        local.get 3
                        local.get 6
                        i32.store
                        i32.const 0
                        local.get 5
                        local.get 8
                        i32.or
                        i32.store offset=1059048
                        local.get 6
                        local.get 3
                        i32.store offset=24
                        local.get 6
                        local.get 6
                        i32.store offset=8
                        local.get 6
                        local.get 6
                        i32.store offset=12
                        br 3 (;@7;)
                      end
                      local.get 0
                      i32.const 0
                      i32.const 25
                      local.get 4
                      i32.const 1
                      i32.shr_u
                      i32.sub
                      local.get 4
                      i32.const 31
                      i32.eq
                      select
                      i32.shl
                      local.set 4
                      local.get 3
                      i32.load
                      local.set 5
                      loop  ;; label = @10
                        local.get 5
                        local.tee 3
                        i32.load offset=4
                        i32.const -8
                        i32.and
                        local.get 0
                        i32.eq
                        br_if 2 (;@8;)
                        local.get 4
                        i32.const 29
                        i32.shr_u
                        local.set 5
                        local.get 4
                        i32.const 1
                        i32.shl
                        local.set 4
                        local.get 3
                        local.get 5
                        i32.const 4
                        i32.and
                        i32.add
                        i32.const 16
                        i32.add
                        local.tee 8
                        i32.load
                        local.tee 5
                        br_if 0 (;@10;)
                      end
                      local.get 8
                      local.get 6
                      i32.store
                      local.get 6
                      local.get 3
                      i32.store offset=24
                      local.get 6
                      local.get 6
                      i32.store offset=12
                      local.get 6
                      local.get 6
                      i32.store offset=8
                      br 2 (;@7;)
                    end
                    local.get 5
                    i32.const -8
                    local.get 5
                    i32.sub
                    i32.const 15
                    i32.and
                    i32.const 0
                    local.get 5
                    i32.const 8
                    i32.add
                    i32.const 15
                    i32.and
                    select
                    local.tee 0
                    i32.add
                    local.tee 11
                    local.get 2
                    i32.const -56
                    i32.add
                    local.tee 8
                    local.get 0
                    i32.sub
                    local.tee 0
                    i32.const 1
                    i32.or
                    i32.store offset=4
                    local.get 5
                    local.get 8
                    i32.add
                    i32.const 56
                    i32.store offset=4
                    local.get 4
                    local.get 6
                    i32.const 55
                    local.get 6
                    i32.sub
                    i32.const 15
                    i32.and
                    i32.const 0
                    local.get 6
                    i32.const -55
                    i32.add
                    i32.const 15
                    i32.and
                    select
                    i32.add
                    i32.const -63
                    i32.add
                    local.tee 8
                    local.get 8
                    local.get 4
                    i32.const 16
                    i32.add
                    i32.lt_u
                    select
                    local.tee 8
                    i32.const 35
                    i32.store offset=4
                    i32.const 0
                    i32.const 0
                    i32.load offset=1059532
                    i32.store offset=1059072
                    i32.const 0
                    local.get 0
                    i32.store offset=1059056
                    i32.const 0
                    local.get 11
                    i32.store offset=1059068
                    local.get 8
                    i32.const 16
                    i32.add
                    i32.const 0
                    i64.load offset=1059500 align=4
                    i64.store align=4
                    local.get 8
                    i32.const 0
                    i64.load offset=1059492 align=4
                    i64.store offset=8 align=4
                    i32.const 0
                    local.get 8
                    i32.const 8
                    i32.add
                    i32.store offset=1059500
                    i32.const 0
                    local.get 2
                    i32.store offset=1059496
                    i32.const 0
                    local.get 5
                    i32.store offset=1059492
                    i32.const 0
                    i32.const 0
                    i32.store offset=1059504
                    local.get 8
                    i32.const 36
                    i32.add
                    local.set 0
                    loop  ;; label = @9
                      local.get 0
                      i32.const 7
                      i32.store
                      local.get 0
                      i32.const 4
                      i32.add
                      local.tee 0
                      local.get 6
                      i32.lt_u
                      br_if 0 (;@9;)
                    end
                    local.get 8
                    local.get 4
                    i32.eq
                    br_if 3 (;@5;)
                    local.get 8
                    local.get 8
                    i32.load offset=4
                    i32.const -2
                    i32.and
                    i32.store offset=4
                    local.get 8
                    local.get 8
                    local.get 4
                    i32.sub
                    local.tee 2
                    i32.store
                    local.get 4
                    local.get 2
                    i32.const 1
                    i32.or
                    i32.store offset=4
                    block  ;; label = @9
                      local.get 2
                      i32.const 255
                      i32.gt_u
                      br_if 0 (;@9;)
                      local.get 2
                      i32.const 3
                      i32.shr_u
                      local.tee 6
                      i32.const 3
                      i32.shl
                      i32.const 1059084
                      i32.add
                      local.set 0
                      block  ;; label = @10
                        block  ;; label = @11
                          i32.const 0
                          i32.load offset=1059044
                          local.tee 5
                          i32.const 1
                          local.get 6
                          i32.shl
                          local.tee 6
                          i32.and
                          br_if 0 (;@11;)
                          i32.const 0
                          local.get 5
                          local.get 6
                          i32.or
                          i32.store offset=1059044
                          local.get 0
                          local.set 6
                          br 1 (;@10;)
                        end
                        local.get 0
                        i32.load offset=8
                        local.set 6
                      end
                      local.get 6
                      local.get 4
                      i32.store offset=12
                      local.get 0
                      local.get 4
                      i32.store offset=8
                      local.get 4
                      local.get 0
                      i32.store offset=12
                      local.get 4
                      local.get 6
                      i32.store offset=8
                      br 4 (;@5;)
                    end
                    i32.const 0
                    local.set 0
                    block  ;; label = @9
                      local.get 2
                      i32.const 8
                      i32.shr_u
                      local.tee 6
                      i32.eqz
                      br_if 0 (;@9;)
                      i32.const 31
                      local.set 0
                      local.get 2
                      i32.const 16777215
                      i32.gt_u
                      br_if 0 (;@9;)
                      local.get 6
                      local.get 6
                      i32.const 1048320
                      i32.add
                      i32.const 16
                      i32.shr_u
                      i32.const 8
                      i32.and
                      local.tee 0
                      i32.shl
                      local.tee 6
                      local.get 6
                      i32.const 520192
                      i32.add
                      i32.const 16
                      i32.shr_u
                      i32.const 4
                      i32.and
                      local.tee 6
                      i32.shl
                      local.tee 5
                      local.get 5
                      i32.const 245760
                      i32.add
                      i32.const 16
                      i32.shr_u
                      i32.const 2
                      i32.and
                      local.tee 5
                      i32.shl
                      i32.const 15
                      i32.shr_u
                      local.get 6
                      local.get 0
                      i32.or
                      local.get 5
                      i32.or
                      i32.sub
                      local.tee 0
                      i32.const 1
                      i32.shl
                      local.get 2
                      local.get 0
                      i32.const 21
                      i32.add
                      i32.shr_u
                      i32.const 1
                      i32.and
                      i32.or
                      i32.const 28
                      i32.add
                      local.set 0
                    end
                    local.get 4
                    i64.const 0
                    i64.store offset=16 align=4
                    local.get 4
                    i32.const 28
                    i32.add
                    local.get 0
                    i32.store
                    local.get 0
                    i32.const 2
                    i32.shl
                    i32.const 1059348
                    i32.add
                    local.set 6
                    block  ;; label = @9
                      i32.const 0
                      i32.load offset=1059048
                      local.tee 5
                      i32.const 1
                      local.get 0
                      i32.shl
                      local.tee 8
                      i32.and
                      br_if 0 (;@9;)
                      local.get 6
                      local.get 4
                      i32.store
                      i32.const 0
                      local.get 5
                      local.get 8
                      i32.or
                      i32.store offset=1059048
                      local.get 4
                      i32.const 24
                      i32.add
                      local.get 6
                      i32.store
                      local.get 4
                      local.get 4
                      i32.store offset=8
                      local.get 4
                      local.get 4
                      i32.store offset=12
                      br 4 (;@5;)
                    end
                    local.get 2
                    i32.const 0
                    i32.const 25
                    local.get 0
                    i32.const 1
                    i32.shr_u
                    i32.sub
                    local.get 0
                    i32.const 31
                    i32.eq
                    select
                    i32.shl
                    local.set 0
                    local.get 6
                    i32.load
                    local.set 5
                    loop  ;; label = @9
                      local.get 5
                      local.tee 6
                      i32.load offset=4
                      i32.const -8
                      i32.and
                      local.get 2
                      i32.eq
                      br_if 3 (;@6;)
                      local.get 0
                      i32.const 29
                      i32.shr_u
                      local.set 5
                      local.get 0
                      i32.const 1
                      i32.shl
                      local.set 0
                      local.get 6
                      local.get 5
                      i32.const 4
                      i32.and
                      i32.add
                      i32.const 16
                      i32.add
                      local.tee 8
                      i32.load
                      local.tee 5
                      br_if 0 (;@9;)
                    end
                    local.get 8
                    local.get 4
                    i32.store
                    local.get 4
                    i32.const 24
                    i32.add
                    local.get 6
                    i32.store
                    local.get 4
                    local.get 4
                    i32.store offset=12
                    local.get 4
                    local.get 4
                    i32.store offset=8
                    br 3 (;@5;)
                  end
                  local.get 3
                  i32.load offset=8
                  local.set 0
                  local.get 3
                  local.get 6
                  i32.store offset=8
                  local.get 0
                  local.get 6
                  i32.store offset=12
                  local.get 6
                  i32.const 0
                  i32.store offset=24
                  local.get 6
                  local.get 0
                  i32.store offset=8
                  local.get 6
                  local.get 3
                  i32.store offset=12
                end
                local.get 11
                i32.const 8
                i32.add
                local.set 0
                br 5 (;@1;)
              end
              local.get 6
              i32.load offset=8
              local.set 0
              local.get 6
              local.get 4
              i32.store offset=8
              local.get 0
              local.get 4
              i32.store offset=12
              local.get 4
              i32.const 24
              i32.add
              i32.const 0
              i32.store
              local.get 4
              local.get 0
              i32.store offset=8
              local.get 4
              local.get 6
              i32.store offset=12
            end
            i32.const 0
            i32.load offset=1059056
            local.tee 0
            local.get 3
            i32.le_u
            br_if 0 (;@4;)
            i32.const 0
            i32.load offset=1059068
            local.tee 4
            local.get 3
            i32.add
            local.tee 6
            local.get 0
            local.get 3
            i32.sub
            local.tee 0
            i32.const 1
            i32.or
            i32.store offset=4
            i32.const 0
            local.get 0
            i32.store offset=1059056
            i32.const 0
            local.get 6
            i32.store offset=1059068
            local.get 4
            local.get 3
            i32.const 3
            i32.or
            i32.store offset=4
            local.get 4
            i32.const 8
            i32.add
            local.set 0
            br 3 (;@1;)
          end
          i32.const 0
          local.set 0
          i32.const 0
          i32.const 48
          i32.store offset=1059540
          br 2 (;@1;)
        end
        block  ;; label = @3
          local.get 11
          i32.eqz
          br_if 0 (;@3;)
          block  ;; label = @4
            block  ;; label = @5
              local.get 8
              local.get 8
              i32.load offset=28
              local.tee 4
              i32.const 2
              i32.shl
              i32.const 1059348
              i32.add
              local.tee 0
              i32.load
              i32.ne
              br_if 0 (;@5;)
              local.get 0
              local.get 5
              i32.store
              local.get 5
              br_if 1 (;@4;)
              i32.const 0
              local.get 7
              i32.const -2
              local.get 4
              i32.rotl
              i32.and
              local.tee 7
              i32.store offset=1059048
              br 2 (;@3;)
            end
            local.get 11
            i32.const 16
            i32.const 20
            local.get 11
            i32.load offset=16
            local.get 8
            i32.eq
            select
            i32.add
            local.get 5
            i32.store
            local.get 5
            i32.eqz
            br_if 1 (;@3;)
          end
          local.get 5
          local.get 11
          i32.store offset=24
          block  ;; label = @4
            local.get 8
            i32.load offset=16
            local.tee 0
            i32.eqz
            br_if 0 (;@4;)
            local.get 5
            local.get 0
            i32.store offset=16
            local.get 0
            local.get 5
            i32.store offset=24
          end
          local.get 8
          i32.const 20
          i32.add
          i32.load
          local.tee 0
          i32.eqz
          br_if 0 (;@3;)
          local.get 5
          i32.const 20
          i32.add
          local.get 0
          i32.store
          local.get 0
          local.get 5
          i32.store offset=24
        end
        block  ;; label = @3
          block  ;; label = @4
            local.get 6
            i32.const 15
            i32.gt_u
            br_if 0 (;@4;)
            local.get 8
            local.get 6
            local.get 3
            i32.add
            local.tee 0
            i32.const 3
            i32.or
            i32.store offset=4
            local.get 8
            local.get 0
            i32.add
            local.tee 0
            local.get 0
            i32.load offset=4
            i32.const 1
            i32.or
            i32.store offset=4
            br 1 (;@3;)
          end
          local.get 8
          local.get 3
          i32.add
          local.tee 5
          local.get 6
          i32.const 1
          i32.or
          i32.store offset=4
          local.get 8
          local.get 3
          i32.const 3
          i32.or
          i32.store offset=4
          local.get 5
          local.get 6
          i32.add
          local.get 6
          i32.store
          block  ;; label = @4
            local.get 6
            i32.const 255
            i32.gt_u
            br_if 0 (;@4;)
            local.get 6
            i32.const 3
            i32.shr_u
            local.tee 4
            i32.const 3
            i32.shl
            i32.const 1059084
            i32.add
            local.set 0
            block  ;; label = @5
              block  ;; label = @6
                i32.const 0
                i32.load offset=1059044
                local.tee 6
                i32.const 1
                local.get 4
                i32.shl
                local.tee 4
                i32.and
                br_if 0 (;@6;)
                i32.const 0
                local.get 6
                local.get 4
                i32.or
                i32.store offset=1059044
                local.get 0
                local.set 4
                br 1 (;@5;)
              end
              local.get 0
              i32.load offset=8
              local.set 4
            end
            local.get 4
            local.get 5
            i32.store offset=12
            local.get 0
            local.get 5
            i32.store offset=8
            local.get 5
            local.get 0
            i32.store offset=12
            local.get 5
            local.get 4
            i32.store offset=8
            br 1 (;@3;)
          end
          block  ;; label = @4
            block  ;; label = @5
              local.get 6
              i32.const 8
              i32.shr_u
              local.tee 4
              br_if 0 (;@5;)
              i32.const 0
              local.set 0
              br 1 (;@4;)
            end
            i32.const 31
            local.set 0
            local.get 6
            i32.const 16777215
            i32.gt_u
            br_if 0 (;@4;)
            local.get 4
            local.get 4
            i32.const 1048320
            i32.add
            i32.const 16
            i32.shr_u
            i32.const 8
            i32.and
            local.tee 0
            i32.shl
            local.tee 4
            local.get 4
            i32.const 520192
            i32.add
            i32.const 16
            i32.shr_u
            i32.const 4
            i32.and
            local.tee 4
            i32.shl
            local.tee 3
            local.get 3
            i32.const 245760
            i32.add
            i32.const 16
            i32.shr_u
            i32.const 2
            i32.and
            local.tee 3
            i32.shl
            i32.const 15
            i32.shr_u
            local.get 4
            local.get 0
            i32.or
            local.get 3
            i32.or
            i32.sub
            local.tee 0
            i32.const 1
            i32.shl
            local.get 6
            local.get 0
            i32.const 21
            i32.add
            i32.shr_u
            i32.const 1
            i32.and
            i32.or
            i32.const 28
            i32.add
            local.set 0
          end
          local.get 5
          local.get 0
          i32.store offset=28
          local.get 5
          i64.const 0
          i64.store offset=16 align=4
          local.get 0
          i32.const 2
          i32.shl
          i32.const 1059348
          i32.add
          local.set 4
          block  ;; label = @4
            local.get 7
            i32.const 1
            local.get 0
            i32.shl
            local.tee 3
            i32.and
            br_if 0 (;@4;)
            local.get 4
            local.get 5
            i32.store
            i32.const 0
            local.get 7
            local.get 3
            i32.or
            i32.store offset=1059048
            local.get 5
            local.get 4
            i32.store offset=24
            local.get 5
            local.get 5
            i32.store offset=8
            local.get 5
            local.get 5
            i32.store offset=12
            br 1 (;@3;)
          end
          local.get 6
          i32.const 0
          i32.const 25
          local.get 0
          i32.const 1
          i32.shr_u
          i32.sub
          local.get 0
          i32.const 31
          i32.eq
          select
          i32.shl
          local.set 0
          local.get 4
          i32.load
          local.set 3
          block  ;; label = @4
            loop  ;; label = @5
              local.get 3
              local.tee 4
              i32.load offset=4
              i32.const -8
              i32.and
              local.get 6
              i32.eq
              br_if 1 (;@4;)
              local.get 0
              i32.const 29
              i32.shr_u
              local.set 3
              local.get 0
              i32.const 1
              i32.shl
              local.set 0
              local.get 4
              local.get 3
              i32.const 4
              i32.and
              i32.add
              i32.const 16
              i32.add
              local.tee 2
              i32.load
              local.tee 3
              br_if 0 (;@5;)
            end
            local.get 2
            local.get 5
            i32.store
            local.get 5
            local.get 4
            i32.store offset=24
            local.get 5
            local.get 5
            i32.store offset=12
            local.get 5
            local.get 5
            i32.store offset=8
            br 1 (;@3;)
          end
          local.get 4
          i32.load offset=8
          local.set 0
          local.get 4
          local.get 5
          i32.store offset=8
          local.get 0
          local.get 5
          i32.store offset=12
          local.get 5
          i32.const 0
          i32.store offset=24
          local.get 5
          local.get 0
          i32.store offset=8
          local.get 5
          local.get 4
          i32.store offset=12
        end
        local.get 8
        i32.const 8
        i32.add
        local.set 0
        br 1 (;@1;)
      end
      block  ;; label = @2
        local.get 10
        i32.eqz
        br_if 0 (;@2;)
        block  ;; label = @3
          block  ;; label = @4
            local.get 5
            local.get 5
            i32.load offset=28
            local.tee 6
            i32.const 2
            i32.shl
            i32.const 1059348
            i32.add
            local.tee 0
            i32.load
            i32.ne
            br_if 0 (;@4;)
            local.get 0
            local.get 8
            i32.store
            local.get 8
            br_if 1 (;@3;)
            i32.const 0
            local.get 9
            i32.const -2
            local.get 6
            i32.rotl
            i32.and
            i32.store offset=1059048
            br 2 (;@2;)
          end
          local.get 10
          i32.const 16
          i32.const 20
          local.get 10
          i32.load offset=16
          local.get 5
          i32.eq
          select
          i32.add
          local.get 8
          i32.store
          local.get 8
          i32.eqz
          br_if 1 (;@2;)
        end
        local.get 8
        local.get 10
        i32.store offset=24
        block  ;; label = @3
          local.get 5
          i32.load offset=16
          local.tee 0
          i32.eqz
          br_if 0 (;@3;)
          local.get 8
          local.get 0
          i32.store offset=16
          local.get 0
          local.get 8
          i32.store offset=24
        end
        local.get 5
        i32.const 20
        i32.add
        i32.load
        local.tee 0
        i32.eqz
        br_if 0 (;@2;)
        local.get 8
        i32.const 20
        i32.add
        local.get 0
        i32.store
        local.get 0
        local.get 8
        i32.store offset=24
      end
      block  ;; label = @2
        block  ;; label = @3
          local.get 4
          i32.const 15
          i32.gt_u
          br_if 0 (;@3;)
          local.get 5
          local.get 4
          local.get 3
          i32.add
          local.tee 0
          i32.const 3
          i32.or
          i32.store offset=4
          local.get 5
          local.get 0
          i32.add
          local.tee 0
          local.get 0
          i32.load offset=4
          i32.const 1
          i32.or
          i32.store offset=4
          br 1 (;@2;)
        end
        local.get 5
        local.get 3
        i32.add
        local.tee 6
        local.get 4
        i32.const 1
        i32.or
        i32.store offset=4
        local.get 5
        local.get 3
        i32.const 3
        i32.or
        i32.store offset=4
        local.get 6
        local.get 4
        i32.add
        local.get 4
        i32.store
        block  ;; label = @3
          local.get 7
          i32.eqz
          br_if 0 (;@3;)
          local.get 7
          i32.const 3
          i32.shr_u
          local.tee 8
          i32.const 3
          i32.shl
          i32.const 1059084
          i32.add
          local.set 3
          i32.const 0
          i32.load offset=1059064
          local.set 0
          block  ;; label = @4
            block  ;; label = @5
              i32.const 1
              local.get 8
              i32.shl
              local.tee 8
              local.get 2
              i32.and
              br_if 0 (;@5;)
              i32.const 0
              local.get 8
              local.get 2
              i32.or
              i32.store offset=1059044
              local.get 3
              local.set 8
              br 1 (;@4;)
            end
            local.get 3
            i32.load offset=8
            local.set 8
          end
          local.get 8
          local.get 0
          i32.store offset=12
          local.get 3
          local.get 0
          i32.store offset=8
          local.get 0
          local.get 3
          i32.store offset=12
          local.get 0
          local.get 8
          i32.store offset=8
        end
        i32.const 0
        local.get 6
        i32.store offset=1059064
        i32.const 0
        local.get 4
        i32.store offset=1059052
      end
      local.get 5
      i32.const 8
      i32.add
      local.set 0
    end
    local.get 1
    i32.const 16
    i32.add
    global.set 0
    local.get 0)
  (func $free (type 0) (param i32)
    local.get 0
    call $dlfree)
  (func $dlfree (type 0) (param i32)
    (local i32 i32 i32 i32 i32 i32 i32)
    block  ;; label = @1
      local.get 0
      i32.eqz
      br_if 0 (;@1;)
      local.get 0
      i32.const -8
      i32.add
      local.tee 1
      local.get 0
      i32.const -4
      i32.add
      i32.load
      local.tee 2
      i32.const -8
      i32.and
      local.tee 0
      i32.add
      local.set 3
      block  ;; label = @2
        local.get 2
        i32.const 1
        i32.and
        br_if 0 (;@2;)
        local.get 2
        i32.const 3
        i32.and
        i32.eqz
        br_if 1 (;@1;)
        local.get 1
        local.get 1
        i32.load
        local.tee 2
        i32.sub
        local.tee 1
        i32.const 0
        i32.load offset=1059060
        local.tee 4
        i32.lt_u
        br_if 1 (;@1;)
        local.get 2
        local.get 0
        i32.add
        local.set 0
        block  ;; label = @3
          i32.const 0
          i32.load offset=1059064
          local.get 1
          i32.eq
          br_if 0 (;@3;)
          block  ;; label = @4
            local.get 2
            i32.const 255
            i32.gt_u
            br_if 0 (;@4;)
            local.get 1
            i32.load offset=12
            local.set 5
            block  ;; label = @5
              local.get 1
              i32.load offset=8
              local.tee 6
              local.get 2
              i32.const 3
              i32.shr_u
              local.tee 7
              i32.const 3
              i32.shl
              i32.const 1059084
              i32.add
              local.tee 2
              i32.eq
              br_if 0 (;@5;)
              local.get 4
              local.get 6
              i32.gt_u
              drop
            end
            block  ;; label = @5
              local.get 5
              local.get 6
              i32.ne
              br_if 0 (;@5;)
              i32.const 0
              i32.const 0
              i32.load offset=1059044
              i32.const -2
              local.get 7
              i32.rotl
              i32.and
              i32.store offset=1059044
              br 3 (;@2;)
            end
            block  ;; label = @5
              local.get 5
              local.get 2
              i32.eq
              br_if 0 (;@5;)
              local.get 4
              local.get 5
              i32.gt_u
              drop
            end
            local.get 5
            local.get 6
            i32.store offset=8
            local.get 6
            local.get 5
            i32.store offset=12
            br 2 (;@2;)
          end
          local.get 1
          i32.load offset=24
          local.set 7
          block  ;; label = @4
            block  ;; label = @5
              local.get 1
              i32.load offset=12
              local.tee 5
              local.get 1
              i32.eq
              br_if 0 (;@5;)
              block  ;; label = @6
                local.get 4
                local.get 1
                i32.load offset=8
                local.tee 2
                i32.gt_u
                br_if 0 (;@6;)
                local.get 2
                i32.load offset=12
                local.get 1
                i32.ne
                drop
              end
              local.get 5
              local.get 2
              i32.store offset=8
              local.get 2
              local.get 5
              i32.store offset=12
              br 1 (;@4;)
            end
            block  ;; label = @5
              local.get 1
              i32.const 20
              i32.add
              local.tee 2
              i32.load
              local.tee 4
              br_if 0 (;@5;)
              local.get 1
              i32.const 16
              i32.add
              local.tee 2
              i32.load
              local.tee 4
              br_if 0 (;@5;)
              i32.const 0
              local.set 5
              br 1 (;@4;)
            end
            loop  ;; label = @5
              local.get 2
              local.set 6
              local.get 4
              local.tee 5
              i32.const 20
              i32.add
              local.tee 2
              i32.load
              local.tee 4
              br_if 0 (;@5;)
              local.get 5
              i32.const 16
              i32.add
              local.set 2
              local.get 5
              i32.load offset=16
              local.tee 4
              br_if 0 (;@5;)
            end
            local.get 6
            i32.const 0
            i32.store
          end
          local.get 7
          i32.eqz
          br_if 1 (;@2;)
          block  ;; label = @4
            block  ;; label = @5
              local.get 1
              i32.load offset=28
              local.tee 4
              i32.const 2
              i32.shl
              i32.const 1059348
              i32.add
              local.tee 2
              i32.load
              local.get 1
              i32.ne
              br_if 0 (;@5;)
              local.get 2
              local.get 5
              i32.store
              local.get 5
              br_if 1 (;@4;)
              i32.const 0
              i32.const 0
              i32.load offset=1059048
              i32.const -2
              local.get 4
              i32.rotl
              i32.and
              i32.store offset=1059048
              br 3 (;@2;)
            end
            local.get 7
            i32.const 16
            i32.const 20
            local.get 7
            i32.load offset=16
            local.get 1
            i32.eq
            select
            i32.add
            local.get 5
            i32.store
            local.get 5
            i32.eqz
            br_if 2 (;@2;)
          end
          local.get 5
          local.get 7
          i32.store offset=24
          block  ;; label = @4
            local.get 1
            i32.load offset=16
            local.tee 2
            i32.eqz
            br_if 0 (;@4;)
            local.get 5
            local.get 2
            i32.store offset=16
            local.get 2
            local.get 5
            i32.store offset=24
          end
          local.get 1
          i32.load offset=20
          local.tee 2
          i32.eqz
          br_if 1 (;@2;)
          local.get 5
          i32.const 20
          i32.add
          local.get 2
          i32.store
          local.get 2
          local.get 5
          i32.store offset=24
          br 1 (;@2;)
        end
        local.get 3
        i32.load offset=4
        local.tee 2
        i32.const 3
        i32.and
        i32.const 3
        i32.ne
        br_if 0 (;@2;)
        local.get 3
        local.get 2
        i32.const -2
        i32.and
        i32.store offset=4
        i32.const 0
        local.get 0
        i32.store offset=1059052
        local.get 1
        local.get 0
        i32.add
        local.get 0
        i32.store
        local.get 1
        local.get 0
        i32.const 1
        i32.or
        i32.store offset=4
        return
      end
      local.get 3
      local.get 1
      i32.le_u
      br_if 0 (;@1;)
      local.get 3
      i32.load offset=4
      local.tee 2
      i32.const 1
      i32.and
      i32.eqz
      br_if 0 (;@1;)
      block  ;; label = @2
        block  ;; label = @3
          local.get 2
          i32.const 2
          i32.and
          br_if 0 (;@3;)
          block  ;; label = @4
            i32.const 0
            i32.load offset=1059068
            local.get 3
            i32.ne
            br_if 0 (;@4;)
            i32.const 0
            local.get 1
            i32.store offset=1059068
            i32.const 0
            i32.const 0
            i32.load offset=1059056
            local.get 0
            i32.add
            local.tee 0
            i32.store offset=1059056
            local.get 1
            local.get 0
            i32.const 1
            i32.or
            i32.store offset=4
            local.get 1
            i32.const 0
            i32.load offset=1059064
            i32.ne
            br_if 3 (;@1;)
            i32.const 0
            i32.const 0
            i32.store offset=1059052
            i32.const 0
            i32.const 0
            i32.store offset=1059064
            return
          end
          block  ;; label = @4
            i32.const 0
            i32.load offset=1059064
            local.get 3
            i32.ne
            br_if 0 (;@4;)
            i32.const 0
            local.get 1
            i32.store offset=1059064
            i32.const 0
            i32.const 0
            i32.load offset=1059052
            local.get 0
            i32.add
            local.tee 0
            i32.store offset=1059052
            local.get 1
            local.get 0
            i32.const 1
            i32.or
            i32.store offset=4
            local.get 1
            local.get 0
            i32.add
            local.get 0
            i32.store
            return
          end
          local.get 2
          i32.const -8
          i32.and
          local.get 0
          i32.add
          local.set 0
          block  ;; label = @4
            block  ;; label = @5
              local.get 2
              i32.const 255
              i32.gt_u
              br_if 0 (;@5;)
              local.get 3
              i32.load offset=12
              local.set 4
              block  ;; label = @6
                local.get 3
                i32.load offset=8
                local.tee 5
                local.get 2
                i32.const 3
                i32.shr_u
                local.tee 3
                i32.const 3
                i32.shl
                i32.const 1059084
                i32.add
                local.tee 2
                i32.eq
                br_if 0 (;@6;)
                i32.const 0
                i32.load offset=1059060
                local.get 5
                i32.gt_u
                drop
              end
              block  ;; label = @6
                local.get 4
                local.get 5
                i32.ne
                br_if 0 (;@6;)
                i32.const 0
                i32.const 0
                i32.load offset=1059044
                i32.const -2
                local.get 3
                i32.rotl
                i32.and
                i32.store offset=1059044
                br 2 (;@4;)
              end
              block  ;; label = @6
                local.get 4
                local.get 2
                i32.eq
                br_if 0 (;@6;)
                i32.const 0
                i32.load offset=1059060
                local.get 4
                i32.gt_u
                drop
              end
              local.get 4
              local.get 5
              i32.store offset=8
              local.get 5
              local.get 4
              i32.store offset=12
              br 1 (;@4;)
            end
            local.get 3
            i32.load offset=24
            local.set 7
            block  ;; label = @5
              block  ;; label = @6
                local.get 3
                i32.load offset=12
                local.tee 5
                local.get 3
                i32.eq
                br_if 0 (;@6;)
                block  ;; label = @7
                  i32.const 0
                  i32.load offset=1059060
                  local.get 3
                  i32.load offset=8
                  local.tee 2
                  i32.gt_u
                  br_if 0 (;@7;)
                  local.get 2
                  i32.load offset=12
                  local.get 3
                  i32.ne
                  drop
                end
                local.get 5
                local.get 2
                i32.store offset=8
                local.get 2
                local.get 5
                i32.store offset=12
                br 1 (;@5;)
              end
              block  ;; label = @6
                local.get 3
                i32.const 20
                i32.add
                local.tee 2
                i32.load
                local.tee 4
                br_if 0 (;@6;)
                local.get 3
                i32.const 16
                i32.add
                local.tee 2
                i32.load
                local.tee 4
                br_if 0 (;@6;)
                i32.const 0
                local.set 5
                br 1 (;@5;)
              end
              loop  ;; label = @6
                local.get 2
                local.set 6
                local.get 4
                local.tee 5
                i32.const 20
                i32.add
                local.tee 2
                i32.load
                local.tee 4
                br_if 0 (;@6;)
                local.get 5
                i32.const 16
                i32.add
                local.set 2
                local.get 5
                i32.load offset=16
                local.tee 4
                br_if 0 (;@6;)
              end
              local.get 6
              i32.const 0
              i32.store
            end
            local.get 7
            i32.eqz
            br_if 0 (;@4;)
            block  ;; label = @5
              block  ;; label = @6
                local.get 3
                i32.load offset=28
                local.tee 4
                i32.const 2
                i32.shl
                i32.const 1059348
                i32.add
                local.tee 2
                i32.load
                local.get 3
                i32.ne
                br_if 0 (;@6;)
                local.get 2
                local.get 5
                i32.store
                local.get 5
                br_if 1 (;@5;)
                i32.const 0
                i32.const 0
                i32.load offset=1059048
                i32.const -2
                local.get 4
                i32.rotl
                i32.and
                i32.store offset=1059048
                br 2 (;@4;)
              end
              local.get 7
              i32.const 16
              i32.const 20
              local.get 7
              i32.load offset=16
              local.get 3
              i32.eq
              select
              i32.add
              local.get 5
              i32.store
              local.get 5
              i32.eqz
              br_if 1 (;@4;)
            end
            local.get 5
            local.get 7
            i32.store offset=24
            block  ;; label = @5
              local.get 3
              i32.load offset=16
              local.tee 2
              i32.eqz
              br_if 0 (;@5;)
              local.get 5
              local.get 2
              i32.store offset=16
              local.get 2
              local.get 5
              i32.store offset=24
            end
            local.get 3
            i32.load offset=20
            local.tee 2
            i32.eqz
            br_if 0 (;@4;)
            local.get 5
            i32.const 20
            i32.add
            local.get 2
            i32.store
            local.get 2
            local.get 5
            i32.store offset=24
          end
          local.get 1
          local.get 0
          i32.add
          local.get 0
          i32.store
          local.get 1
          local.get 0
          i32.const 1
          i32.or
          i32.store offset=4
          local.get 1
          i32.const 0
          i32.load offset=1059064
          i32.ne
          br_if 1 (;@2;)
          i32.const 0
          local.get 0
          i32.store offset=1059052
          return
        end
        local.get 3
        local.get 2
        i32.const -2
        i32.and
        i32.store offset=4
        local.get 1
        local.get 0
        i32.add
        local.get 0
        i32.store
        local.get 1
        local.get 0
        i32.const 1
        i32.or
        i32.store offset=4
      end
      block  ;; label = @2
        local.get 0
        i32.const 255
        i32.gt_u
        br_if 0 (;@2;)
        local.get 0
        i32.const 3
        i32.shr_u
        local.tee 2
        i32.const 3
        i32.shl
        i32.const 1059084
        i32.add
        local.set 0
        block  ;; label = @3
          block  ;; label = @4
            i32.const 0
            i32.load offset=1059044
            local.tee 4
            i32.const 1
            local.get 2
            i32.shl
            local.tee 2
            i32.and
            br_if 0 (;@4;)
            i32.const 0
            local.get 4
            local.get 2
            i32.or
            i32.store offset=1059044
            local.get 0
            local.set 2
            br 1 (;@3;)
          end
          local.get 0
          i32.load offset=8
          local.set 2
        end
        local.get 2
        local.get 1
        i32.store offset=12
        local.get 0
        local.get 1
        i32.store offset=8
        local.get 1
        local.get 0
        i32.store offset=12
        local.get 1
        local.get 2
        i32.store offset=8
        return
      end
      i32.const 0
      local.set 2
      block  ;; label = @2
        local.get 0
        i32.const 8
        i32.shr_u
        local.tee 4
        i32.eqz
        br_if 0 (;@2;)
        i32.const 31
        local.set 2
        local.get 0
        i32.const 16777215
        i32.gt_u
        br_if 0 (;@2;)
        local.get 4
        local.get 4
        i32.const 1048320
        i32.add
        i32.const 16
        i32.shr_u
        i32.const 8
        i32.and
        local.tee 2
        i32.shl
        local.tee 4
        local.get 4
        i32.const 520192
        i32.add
        i32.const 16
        i32.shr_u
        i32.const 4
        i32.and
        local.tee 4
        i32.shl
        local.tee 5
        local.get 5
        i32.const 245760
        i32.add
        i32.const 16
        i32.shr_u
        i32.const 2
        i32.and
        local.tee 5
        i32.shl
        i32.const 15
        i32.shr_u
        local.get 4
        local.get 2
        i32.or
        local.get 5
        i32.or
        i32.sub
        local.tee 2
        i32.const 1
        i32.shl
        local.get 0
        local.get 2
        i32.const 21
        i32.add
        i32.shr_u
        i32.const 1
        i32.and
        i32.or
        i32.const 28
        i32.add
        local.set 2
      end
      local.get 1
      i64.const 0
      i64.store offset=16 align=4
      local.get 1
      i32.const 28
      i32.add
      local.get 2
      i32.store
      local.get 2
      i32.const 2
      i32.shl
      i32.const 1059348
      i32.add
      local.set 4
      block  ;; label = @2
        block  ;; label = @3
          i32.const 0
          i32.load offset=1059048
          local.tee 5
          i32.const 1
          local.get 2
          i32.shl
          local.tee 3
          i32.and
          br_if 0 (;@3;)
          local.get 4
          local.get 1
          i32.store
          i32.const 0
          local.get 5
          local.get 3
          i32.or
          i32.store offset=1059048
          local.get 1
          i32.const 24
          i32.add
          local.get 4
          i32.store
          local.get 1
          local.get 1
          i32.store offset=8
          local.get 1
          local.get 1
          i32.store offset=12
          br 1 (;@2;)
        end
        local.get 0
        i32.const 0
        i32.const 25
        local.get 2
        i32.const 1
        i32.shr_u
        i32.sub
        local.get 2
        i32.const 31
        i32.eq
        select
        i32.shl
        local.set 2
        local.get 4
        i32.load
        local.set 5
        block  ;; label = @3
          loop  ;; label = @4
            local.get 5
            local.tee 4
            i32.load offset=4
            i32.const -8
            i32.and
            local.get 0
            i32.eq
            br_if 1 (;@3;)
            local.get 2
            i32.const 29
            i32.shr_u
            local.set 5
            local.get 2
            i32.const 1
            i32.shl
            local.set 2
            local.get 4
            local.get 5
            i32.const 4
            i32.and
            i32.add
            i32.const 16
            i32.add
            local.tee 3
            i32.load
            local.tee 5
            br_if 0 (;@4;)
          end
          local.get 3
          local.get 1
          i32.store
          local.get 1
          local.get 1
          i32.store offset=12
          local.get 1
          i32.const 24
          i32.add
          local.get 4
          i32.store
          local.get 1
          local.get 1
          i32.store offset=8
          br 1 (;@2;)
        end
        local.get 4
        i32.load offset=8
        local.set 0
        local.get 4
        local.get 1
        i32.store offset=8
        local.get 0
        local.get 1
        i32.store offset=12
        local.get 1
        i32.const 24
        i32.add
        i32.const 0
        i32.store
        local.get 1
        local.get 0
        i32.store offset=8
        local.get 1
        local.get 4
        i32.store offset=12
      end
      i32.const 0
      i32.const 0
      i32.load offset=1059076
      i32.const -1
      i32.add
      local.tee 1
      i32.store offset=1059076
      local.get 1
      br_if 0 (;@1;)
      i32.const 1059500
      local.set 1
      loop  ;; label = @2
        local.get 1
        i32.load
        local.tee 0
        i32.const 8
        i32.add
        local.set 1
        local.get 0
        br_if 0 (;@2;)
      end
      i32.const 0
      i32.const -1
      i32.store offset=1059076
    end)
  (func $realloc (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)
    block  ;; label = @1
      local.get 0
      br_if 0 (;@1;)
      local.get 1
      call $dlmalloc
      return
    end
    block  ;; label = @1
      local.get 1
      i32.const -64
      i32.lt_u
      br_if 0 (;@1;)
      i32.const 0
      i32.const 48
      i32.store offset=1059540
      i32.const 0
      return
    end
    local.get 1
    i32.const 11
    i32.lt_u
    local.set 2
    local.get 1
    i32.const 19
    i32.add
    i32.const -16
    i32.and
    local.set 3
    local.get 0
    i32.const -8
    i32.add
    local.set 4
    local.get 0
    i32.const -4
    i32.add
    local.tee 5
    i32.load
    local.tee 6
    i32.const 3
    i32.and
    local.set 7
    i32.const 0
    i32.load offset=1059060
    local.set 8
    block  ;; label = @1
      local.get 6
      i32.const -8
      i32.and
      local.tee 9
      i32.const 1
      i32.lt_s
      br_if 0 (;@1;)
      local.get 7
      i32.const 1
      i32.eq
      br_if 0 (;@1;)
      local.get 8
      local.get 4
      i32.gt_u
      drop
    end
    i32.const 16
    local.get 3
    local.get 2
    select
    local.set 2
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 7
          br_if 0 (;@3;)
          local.get 2
          i32.const 256
          i32.lt_u
          br_if 1 (;@2;)
          local.get 9
          local.get 2
          i32.const 4
          i32.or
          i32.lt_u
          br_if 1 (;@2;)
          local.get 9
          local.get 2
          i32.sub
          i32.const 0
          i32.load offset=1059524
          i32.const 1
          i32.shl
          i32.le_u
          br_if 2 (;@1;)
          br 1 (;@2;)
        end
        local.get 4
        local.get 9
        i32.add
        local.set 7
        block  ;; label = @3
          local.get 9
          local.get 2
          i32.lt_u
          br_if 0 (;@3;)
          local.get 9
          local.get 2
          i32.sub
          local.tee 1
          i32.const 16
          i32.lt_u
          br_if 2 (;@1;)
          local.get 5
          local.get 2
          local.get 6
          i32.const 1
          i32.and
          i32.or
          i32.const 2
          i32.or
          i32.store
          local.get 4
          local.get 2
          i32.add
          local.tee 2
          local.get 1
          i32.const 3
          i32.or
          i32.store offset=4
          local.get 7
          local.get 7
          i32.load offset=4
          i32.const 1
          i32.or
          i32.store offset=4
          local.get 2
          local.get 1
          call $dispose_chunk
          local.get 0
          return
        end
        block  ;; label = @3
          i32.const 0
          i32.load offset=1059068
          local.get 7
          i32.ne
          br_if 0 (;@3;)
          i32.const 0
          i32.load offset=1059056
          local.get 9
          i32.add
          local.tee 9
          local.get 2
          i32.le_u
          br_if 1 (;@2;)
          local.get 5
          local.get 2
          local.get 6
          i32.const 1
          i32.and
          i32.or
          i32.const 2
          i32.or
          i32.store
          i32.const 0
          local.get 4
          local.get 2
          i32.add
          local.tee 1
          i32.store offset=1059068
          i32.const 0
          local.get 9
          local.get 2
          i32.sub
          local.tee 2
          i32.store offset=1059056
          local.get 1
          local.get 2
          i32.const 1
          i32.or
          i32.store offset=4
          local.get 0
          return
        end
        block  ;; label = @3
          i32.const 0
          i32.load offset=1059064
          local.get 7
          i32.ne
          br_if 0 (;@3;)
          i32.const 0
          i32.load offset=1059052
          local.get 9
          i32.add
          local.tee 9
          local.get 2
          i32.lt_u
          br_if 1 (;@2;)
          block  ;; label = @4
            block  ;; label = @5
              local.get 9
              local.get 2
              i32.sub
              local.tee 1
              i32.const 16
              i32.lt_u
              br_if 0 (;@5;)
              local.get 5
              local.get 2
              local.get 6
              i32.const 1
              i32.and
              i32.or
              i32.const 2
              i32.or
              i32.store
              local.get 4
              local.get 2
              i32.add
              local.tee 2
              local.get 1
              i32.const 1
              i32.or
              i32.store offset=4
              local.get 4
              local.get 9
              i32.add
              local.tee 9
              local.get 1
              i32.store
              local.get 9
              local.get 9
              i32.load offset=4
              i32.const -2
              i32.and
              i32.store offset=4
              br 1 (;@4;)
            end
            local.get 5
            local.get 6
            i32.const 1
            i32.and
            local.get 9
            i32.or
            i32.const 2
            i32.or
            i32.store
            local.get 4
            local.get 9
            i32.add
            local.tee 1
            local.get 1
            i32.load offset=4
            i32.const 1
            i32.or
            i32.store offset=4
            i32.const 0
            local.set 1
            i32.const 0
            local.set 2
          end
          i32.const 0
          local.get 2
          i32.store offset=1059064
          i32.const 0
          local.get 1
          i32.store offset=1059052
          local.get 0
          return
        end
        local.get 7
        i32.load offset=4
        local.tee 3
        i32.const 2
        i32.and
        br_if 0 (;@2;)
        local.get 3
        i32.const -8
        i32.and
        local.get 9
        i32.add
        local.tee 10
        local.get 2
        i32.lt_u
        br_if 0 (;@2;)
        local.get 10
        local.get 2
        i32.sub
        local.set 11
        block  ;; label = @3
          block  ;; label = @4
            local.get 3
            i32.const 255
            i32.gt_u
            br_if 0 (;@4;)
            local.get 7
            i32.load offset=12
            local.set 1
            block  ;; label = @5
              local.get 7
              i32.load offset=8
              local.tee 9
              local.get 3
              i32.const 3
              i32.shr_u
              local.tee 3
              i32.const 3
              i32.shl
              i32.const 1059084
              i32.add
              local.tee 7
              i32.eq
              br_if 0 (;@5;)
              local.get 8
              local.get 9
              i32.gt_u
              drop
            end
            block  ;; label = @5
              local.get 1
              local.get 9
              i32.ne
              br_if 0 (;@5;)
              i32.const 0
              i32.const 0
              i32.load offset=1059044
              i32.const -2
              local.get 3
              i32.rotl
              i32.and
              i32.store offset=1059044
              br 2 (;@3;)
            end
            block  ;; label = @5
              local.get 1
              local.get 7
              i32.eq
              br_if 0 (;@5;)
              local.get 8
              local.get 1
              i32.gt_u
              drop
            end
            local.get 1
            local.get 9
            i32.store offset=8
            local.get 9
            local.get 1
            i32.store offset=12
            br 1 (;@3;)
          end
          local.get 7
          i32.load offset=24
          local.set 12
          block  ;; label = @4
            block  ;; label = @5
              local.get 7
              i32.load offset=12
              local.tee 3
              local.get 7
              i32.eq
              br_if 0 (;@5;)
              block  ;; label = @6
                local.get 8
                local.get 7
                i32.load offset=8
                local.tee 1
                i32.gt_u
                br_if 0 (;@6;)
                local.get 1
                i32.load offset=12
                local.get 7
                i32.ne
                drop
              end
              local.get 3
              local.get 1
              i32.store offset=8
              local.get 1
              local.get 3
              i32.store offset=12
              br 1 (;@4;)
            end
            block  ;; label = @5
              local.get 7
              i32.const 20
              i32.add
              local.tee 1
              i32.load
              local.tee 9
              br_if 0 (;@5;)
              local.get 7
              i32.const 16
              i32.add
              local.tee 1
              i32.load
              local.tee 9
              br_if 0 (;@5;)
              i32.const 0
              local.set 3
              br 1 (;@4;)
            end
            loop  ;; label = @5
              local.get 1
              local.set 8
              local.get 9
              local.tee 3
              i32.const 20
              i32.add
              local.tee 1
              i32.load
              local.tee 9
              br_if 0 (;@5;)
              local.get 3
              i32.const 16
              i32.add
              local.set 1
              local.get 3
              i32.load offset=16
              local.tee 9
              br_if 0 (;@5;)
            end
            local.get 8
            i32.const 0
            i32.store
          end
          local.get 12
          i32.eqz
          br_if 0 (;@3;)
          block  ;; label = @4
            block  ;; label = @5
              local.get 7
              i32.load offset=28
              local.tee 9
              i32.const 2
              i32.shl
              i32.const 1059348
              i32.add
              local.tee 1
              i32.load
              local.get 7
              i32.ne
              br_if 0 (;@5;)
              local.get 1
              local.get 3
              i32.store
              local.get 3
              br_if 1 (;@4;)
              i32.const 0
              i32.const 0
              i32.load offset=1059048
              i32.const -2
              local.get 9
              i32.rotl
              i32.and
              i32.store offset=1059048
              br 2 (;@3;)
            end
            local.get 12
            i32.const 16
            i32.const 20
            local.get 12
            i32.load offset=16
            local.get 7
            i32.eq
            select
            i32.add
            local.get 3
            i32.store
            local.get 3
            i32.eqz
            br_if 1 (;@3;)
          end
          local.get 3
          local.get 12
          i32.store offset=24
          block  ;; label = @4
            local.get 7
            i32.load offset=16
            local.tee 1
            i32.eqz
            br_if 0 (;@4;)
            local.get 3
            local.get 1
            i32.store offset=16
            local.get 1
            local.get 3
            i32.store offset=24
          end
          local.get 7
          i32.load offset=20
          local.tee 1
          i32.eqz
          br_if 0 (;@3;)
          local.get 3
          i32.const 20
          i32.add
          local.get 1
          i32.store
          local.get 1
          local.get 3
          i32.store offset=24
        end
        block  ;; label = @3
          local.get 11
          i32.const 15
          i32.gt_u
          br_if 0 (;@3;)
          local.get 5
          local.get 6
          i32.const 1
          i32.and
          local.get 10
          i32.or
          i32.const 2
          i32.or
          i32.store
          local.get 4
          local.get 10
          i32.add
          local.tee 1
          local.get 1
          i32.load offset=4
          i32.const 1
          i32.or
          i32.store offset=4
          local.get 0
          return
        end
        local.get 5
        local.get 2
        local.get 6
        i32.const 1
        i32.and
        i32.or
        i32.const 2
        i32.or
        i32.store
        local.get 4
        local.get 2
        i32.add
        local.tee 1
        local.get 11
        i32.const 3
        i32.or
        i32.store offset=4
        local.get 4
        local.get 10
        i32.add
        local.tee 2
        local.get 2
        i32.load offset=4
        i32.const 1
        i32.or
        i32.store offset=4
        local.get 1
        local.get 11
        call $dispose_chunk
        local.get 0
        return
      end
      block  ;; label = @2
        local.get 1
        call $dlmalloc
        local.tee 2
        br_if 0 (;@2;)
        i32.const 0
        return
      end
      local.get 2
      local.get 0
      local.get 5
      i32.load
      local.tee 9
      i32.const -8
      i32.and
      i32.const 4
      i32.const 8
      local.get 9
      i32.const 3
      i32.and
      select
      i32.sub
      local.tee 9
      local.get 1
      local.get 9
      local.get 1
      i32.lt_u
      select
      call $memcpy
      local.set 1
      local.get 0
      call $dlfree
      local.get 1
      local.set 0
    end
    local.get 0)
  (func $dispose_chunk (type 4) (param i32 i32)
    (local i32 i32 i32 i32 i32 i32)
    local.get 0
    local.get 1
    i32.add
    local.set 2
    block  ;; label = @1
      block  ;; label = @2
        local.get 0
        i32.load offset=4
        local.tee 3
        i32.const 1
        i32.and
        br_if 0 (;@2;)
        local.get 3
        i32.const 3
        i32.and
        i32.eqz
        br_if 1 (;@1;)
        local.get 0
        i32.load
        local.tee 3
        local.get 1
        i32.add
        local.set 1
        block  ;; label = @3
          i32.const 0
          i32.load offset=1059064
          local.get 0
          local.get 3
          i32.sub
          local.tee 0
          i32.eq
          br_if 0 (;@3;)
          i32.const 0
          i32.load offset=1059060
          local.set 4
          block  ;; label = @4
            local.get 3
            i32.const 255
            i32.gt_u
            br_if 0 (;@4;)
            local.get 0
            i32.load offset=12
            local.set 5
            block  ;; label = @5
              local.get 0
              i32.load offset=8
              local.tee 6
              local.get 3
              i32.const 3
              i32.shr_u
              local.tee 7
              i32.const 3
              i32.shl
              i32.const 1059084
              i32.add
              local.tee 3
              i32.eq
              br_if 0 (;@5;)
              local.get 4
              local.get 6
              i32.gt_u
              drop
            end
            block  ;; label = @5
              local.get 5
              local.get 6
              i32.ne
              br_if 0 (;@5;)
              i32.const 0
              i32.const 0
              i32.load offset=1059044
              i32.const -2
              local.get 7
              i32.rotl
              i32.and
              i32.store offset=1059044
              br 3 (;@2;)
            end
            block  ;; label = @5
              local.get 5
              local.get 3
              i32.eq
              br_if 0 (;@5;)
              local.get 4
              local.get 5
              i32.gt_u
              drop
            end
            local.get 5
            local.get 6
            i32.store offset=8
            local.get 6
            local.get 5
            i32.store offset=12
            br 2 (;@2;)
          end
          local.get 0
          i32.load offset=24
          local.set 7
          block  ;; label = @4
            block  ;; label = @5
              local.get 0
              i32.load offset=12
              local.tee 6
              local.get 0
              i32.eq
              br_if 0 (;@5;)
              block  ;; label = @6
                local.get 4
                local.get 0
                i32.load offset=8
                local.tee 3
                i32.gt_u
                br_if 0 (;@6;)
                local.get 3
                i32.load offset=12
                local.get 0
                i32.ne
                drop
              end
              local.get 6
              local.get 3
              i32.store offset=8
              local.get 3
              local.get 6
              i32.store offset=12
              br 1 (;@4;)
            end
            block  ;; label = @5
              local.get 0
              i32.const 20
              i32.add
              local.tee 3
              i32.load
              local.tee 5
              br_if 0 (;@5;)
              local.get 0
              i32.const 16
              i32.add
              local.tee 3
              i32.load
              local.tee 5
              br_if 0 (;@5;)
              i32.const 0
              local.set 6
              br 1 (;@4;)
            end
            loop  ;; label = @5
              local.get 3
              local.set 4
              local.get 5
              local.tee 6
              i32.const 20
              i32.add
              local.tee 3
              i32.load
              local.tee 5
              br_if 0 (;@5;)
              local.get 6
              i32.const 16
              i32.add
              local.set 3
              local.get 6
              i32.load offset=16
              local.tee 5
              br_if 0 (;@5;)
            end
            local.get 4
            i32.const 0
            i32.store
          end
          local.get 7
          i32.eqz
          br_if 1 (;@2;)
          block  ;; label = @4
            block  ;; label = @5
              local.get 0
              i32.load offset=28
              local.tee 5
              i32.const 2
              i32.shl
              i32.const 1059348
              i32.add
              local.tee 3
              i32.load
              local.get 0
              i32.ne
              br_if 0 (;@5;)
              local.get 3
              local.get 6
              i32.store
              local.get 6
              br_if 1 (;@4;)
              i32.const 0
              i32.const 0
              i32.load offset=1059048
              i32.const -2
              local.get 5
              i32.rotl
              i32.and
              i32.store offset=1059048
              br 3 (;@2;)
            end
            local.get 7
            i32.const 16
            i32.const 20
            local.get 7
            i32.load offset=16
            local.get 0
            i32.eq
            select
            i32.add
            local.get 6
            i32.store
            local.get 6
            i32.eqz
            br_if 2 (;@2;)
          end
          local.get 6
          local.get 7
          i32.store offset=24
          block  ;; label = @4
            local.get 0
            i32.load offset=16
            local.tee 3
            i32.eqz
            br_if 0 (;@4;)
            local.get 6
            local.get 3
            i32.store offset=16
            local.get 3
            local.get 6
            i32.store offset=24
          end
          local.get 0
          i32.load offset=20
          local.tee 3
          i32.eqz
          br_if 1 (;@2;)
          local.get 6
          i32.const 20
          i32.add
          local.get 3
          i32.store
          local.get 3
          local.get 6
          i32.store offset=24
          br 1 (;@2;)
        end
        local.get 2
        i32.load offset=4
        local.tee 3
        i32.const 3
        i32.and
        i32.const 3
        i32.ne
        br_if 0 (;@2;)
        local.get 2
        local.get 3
        i32.const -2
        i32.and
        i32.store offset=4
        i32.const 0
        local.get 1
        i32.store offset=1059052
        local.get 2
        local.get 1
        i32.store
        local.get 0
        local.get 1
        i32.const 1
        i32.or
        i32.store offset=4
        return
      end
      block  ;; label = @2
        block  ;; label = @3
          local.get 2
          i32.load offset=4
          local.tee 3
          i32.const 2
          i32.and
          br_if 0 (;@3;)
          block  ;; label = @4
            i32.const 0
            i32.load offset=1059068
            local.get 2
            i32.ne
            br_if 0 (;@4;)
            i32.const 0
            local.get 0
            i32.store offset=1059068
            i32.const 0
            i32.const 0
            i32.load offset=1059056
            local.get 1
            i32.add
            local.tee 1
            i32.store offset=1059056
            local.get 0
            local.get 1
            i32.const 1
            i32.or
            i32.store offset=4
            local.get 0
            i32.const 0
            i32.load offset=1059064
            i32.ne
            br_if 3 (;@1;)
            i32.const 0
            i32.const 0
            i32.store offset=1059052
            i32.const 0
            i32.const 0
            i32.store offset=1059064
            return
          end
          block  ;; label = @4
            i32.const 0
            i32.load offset=1059064
            local.get 2
            i32.ne
            br_if 0 (;@4;)
            i32.const 0
            local.get 0
            i32.store offset=1059064
            i32.const 0
            i32.const 0
            i32.load offset=1059052
            local.get 1
            i32.add
            local.tee 1
            i32.store offset=1059052
            local.get 0
            local.get 1
            i32.const 1
            i32.or
            i32.store offset=4
            local.get 0
            local.get 1
            i32.add
            local.get 1
            i32.store
            return
          end
          i32.const 0
          i32.load offset=1059060
          local.set 4
          local.get 3
          i32.const -8
          i32.and
          local.get 1
          i32.add
          local.set 1
          block  ;; label = @4
            block  ;; label = @5
              local.get 3
              i32.const 255
              i32.gt_u
              br_if 0 (;@5;)
              local.get 2
              i32.load offset=12
              local.set 5
              block  ;; label = @6
                local.get 2
                i32.load offset=8
                local.tee 6
                local.get 3
                i32.const 3
                i32.shr_u
                local.tee 2
                i32.const 3
                i32.shl
                i32.const 1059084
                i32.add
                local.tee 3
                i32.eq
                br_if 0 (;@6;)
                local.get 4
                local.get 6
                i32.gt_u
                drop
              end
              block  ;; label = @6
                local.get 5
                local.get 6
                i32.ne
                br_if 0 (;@6;)
                i32.const 0
                i32.const 0
                i32.load offset=1059044
                i32.const -2
                local.get 2
                i32.rotl
                i32.and
                i32.store offset=1059044
                br 2 (;@4;)
              end
              block  ;; label = @6
                local.get 5
                local.get 3
                i32.eq
                br_if 0 (;@6;)
                local.get 4
                local.get 5
                i32.gt_u
                drop
              end
              local.get 5
              local.get 6
              i32.store offset=8
              local.get 6
              local.get 5
              i32.store offset=12
              br 1 (;@4;)
            end
            local.get 2
            i32.load offset=24
            local.set 7
            block  ;; label = @5
              block  ;; label = @6
                local.get 2
                i32.load offset=12
                local.tee 6
                local.get 2
                i32.eq
                br_if 0 (;@6;)
                block  ;; label = @7
                  local.get 4
                  local.get 2
                  i32.load offset=8
                  local.tee 3
                  i32.gt_u
                  br_if 0 (;@7;)
                  local.get 3
                  i32.load offset=12
                  local.get 2
                  i32.ne
                  drop
                end
                local.get 6
                local.get 3
                i32.store offset=8
                local.get 3
                local.get 6
                i32.store offset=12
                br 1 (;@5;)
              end
              block  ;; label = @6
                local.get 2
                i32.const 20
                i32.add
                local.tee 3
                i32.load
                local.tee 5
                br_if 0 (;@6;)
                local.get 2
                i32.const 16
                i32.add
                local.tee 3
                i32.load
                local.tee 5
                br_if 0 (;@6;)
                i32.const 0
                local.set 6
                br 1 (;@5;)
              end
              loop  ;; label = @6
                local.get 3
                local.set 4
                local.get 5
                local.tee 6
                i32.const 20
                i32.add
                local.tee 3
                i32.load
                local.tee 5
                br_if 0 (;@6;)
                local.get 6
                i32.const 16
                i32.add
                local.set 3
                local.get 6
                i32.load offset=16
                local.tee 5
                br_if 0 (;@6;)
              end
              local.get 4
              i32.const 0
              i32.store
            end
            local.get 7
            i32.eqz
            br_if 0 (;@4;)
            block  ;; label = @5
              block  ;; label = @6
                local.get 2
                i32.load offset=28
                local.tee 5
                i32.const 2
                i32.shl
                i32.const 1059348
                i32.add
                local.tee 3
                i32.load
                local.get 2
                i32.ne
                br_if 0 (;@6;)
                local.get 3
                local.get 6
                i32.store
                local.get 6
                br_if 1 (;@5;)
                i32.const 0
                i32.const 0
                i32.load offset=1059048
                i32.const -2
                local.get 5
                i32.rotl
                i32.and
                i32.store offset=1059048
                br 2 (;@4;)
              end
              local.get 7
              i32.const 16
              i32.const 20
              local.get 7
              i32.load offset=16
              local.get 2
              i32.eq
              select
              i32.add
              local.get 6
              i32.store
              local.get 6
              i32.eqz
              br_if 1 (;@4;)
            end
            local.get 6
            local.get 7
            i32.store offset=24
            block  ;; label = @5
              local.get 2
              i32.load offset=16
              local.tee 3
              i32.eqz
              br_if 0 (;@5;)
              local.get 6
              local.get 3
              i32.store offset=16
              local.get 3
              local.get 6
              i32.store offset=24
            end
            local.get 2
            i32.load offset=20
            local.tee 3
            i32.eqz
            br_if 0 (;@4;)
            local.get 6
            i32.const 20
            i32.add
            local.get 3
            i32.store
            local.get 3
            local.get 6
            i32.store offset=24
          end
          local.get 0
          local.get 1
          i32.add
          local.get 1
          i32.store
          local.get 0
          local.get 1
          i32.const 1
          i32.or
          i32.store offset=4
          local.get 0
          i32.const 0
          i32.load offset=1059064
          i32.ne
          br_if 1 (;@2;)
          i32.const 0
          local.get 1
          i32.store offset=1059052
          return
        end
        local.get 2
        local.get 3
        i32.const -2
        i32.and
        i32.store offset=4
        local.get 0
        local.get 1
        i32.add
        local.get 1
        i32.store
        local.get 0
        local.get 1
        i32.const 1
        i32.or
        i32.store offset=4
      end
      block  ;; label = @2
        local.get 1
        i32.const 255
        i32.gt_u
        br_if 0 (;@2;)
        local.get 1
        i32.const 3
        i32.shr_u
        local.tee 3
        i32.const 3
        i32.shl
        i32.const 1059084
        i32.add
        local.set 1
        block  ;; label = @3
          block  ;; label = @4
            i32.const 0
            i32.load offset=1059044
            local.tee 5
            i32.const 1
            local.get 3
            i32.shl
            local.tee 3
            i32.and
            br_if 0 (;@4;)
            i32.const 0
            local.get 5
            local.get 3
            i32.or
            i32.store offset=1059044
            local.get 1
            local.set 3
            br 1 (;@3;)
          end
          local.get 1
          i32.load offset=8
          local.set 3
        end
        local.get 3
        local.get 0
        i32.store offset=12
        local.get 1
        local.get 0
        i32.store offset=8
        local.get 0
        local.get 1
        i32.store offset=12
        local.get 0
        local.get 3
        i32.store offset=8
        return
      end
      i32.const 0
      local.set 3
      block  ;; label = @2
        local.get 1
        i32.const 8
        i32.shr_u
        local.tee 5
        i32.eqz
        br_if 0 (;@2;)
        i32.const 31
        local.set 3
        local.get 1
        i32.const 16777215
        i32.gt_u
        br_if 0 (;@2;)
        local.get 5
        local.get 5
        i32.const 1048320
        i32.add
        i32.const 16
        i32.shr_u
        i32.const 8
        i32.and
        local.tee 3
        i32.shl
        local.tee 5
        local.get 5
        i32.const 520192
        i32.add
        i32.const 16
        i32.shr_u
        i32.const 4
        i32.and
        local.tee 5
        i32.shl
        local.tee 6
        local.get 6
        i32.const 245760
        i32.add
        i32.const 16
        i32.shr_u
        i32.const 2
        i32.and
        local.tee 6
        i32.shl
        i32.const 15
        i32.shr_u
        local.get 5
        local.get 3
        i32.or
        local.get 6
        i32.or
        i32.sub
        local.tee 3
        i32.const 1
        i32.shl
        local.get 1
        local.get 3
        i32.const 21
        i32.add
        i32.shr_u
        i32.const 1
        i32.and
        i32.or
        i32.const 28
        i32.add
        local.set 3
      end
      local.get 0
      i64.const 0
      i64.store offset=16 align=4
      local.get 0
      i32.const 28
      i32.add
      local.get 3
      i32.store
      local.get 3
      i32.const 2
      i32.shl
      i32.const 1059348
      i32.add
      local.set 5
      block  ;; label = @2
        i32.const 0
        i32.load offset=1059048
        local.tee 6
        i32.const 1
        local.get 3
        i32.shl
        local.tee 2
        i32.and
        br_if 0 (;@2;)
        local.get 5
        local.get 0
        i32.store
        i32.const 0
        local.get 6
        local.get 2
        i32.or
        i32.store offset=1059048
        local.get 0
        i32.const 24
        i32.add
        local.get 5
        i32.store
        local.get 0
        local.get 0
        i32.store offset=8
        local.get 0
        local.get 0
        i32.store offset=12
        return
      end
      local.get 1
      i32.const 0
      i32.const 25
      local.get 3
      i32.const 1
      i32.shr_u
      i32.sub
      local.get 3
      i32.const 31
      i32.eq
      select
      i32.shl
      local.set 3
      local.get 5
      i32.load
      local.set 6
      block  ;; label = @2
        loop  ;; label = @3
          local.get 6
          local.tee 5
          i32.load offset=4
          i32.const -8
          i32.and
          local.get 1
          i32.eq
          br_if 1 (;@2;)
          local.get 3
          i32.const 29
          i32.shr_u
          local.set 6
          local.get 3
          i32.const 1
          i32.shl
          local.set 3
          local.get 5
          local.get 6
          i32.const 4
          i32.and
          i32.add
          i32.const 16
          i32.add
          local.tee 2
          i32.load
          local.tee 6
          br_if 0 (;@3;)
        end
        local.get 2
        local.get 0
        i32.store
        local.get 0
        i32.const 24
        i32.add
        local.get 5
        i32.store
        local.get 0
        local.get 0
        i32.store offset=12
        local.get 0
        local.get 0
        i32.store offset=8
        return
      end
      local.get 5
      i32.load offset=8
      local.set 1
      local.get 5
      local.get 0
      i32.store offset=8
      local.get 1
      local.get 0
      i32.store offset=12
      local.get 0
      i32.const 24
      i32.add
      i32.const 0
      i32.store
      local.get 0
      local.get 1
      i32.store offset=8
      local.get 0
      local.get 5
      i32.store offset=12
    end)
  (func $internal_memalign (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32 i32 i32)
    block  ;; label = @1
      block  ;; label = @2
        local.get 0
        i32.const 16
        local.get 0
        i32.const 16
        i32.gt_u
        select
        local.tee 2
        local.get 2
        i32.const -1
        i32.add
        i32.and
        br_if 0 (;@2;)
        local.get 2
        local.set 0
        br 1 (;@1;)
      end
      i32.const 32
      local.set 3
      loop  ;; label = @2
        local.get 3
        local.tee 0
        i32.const 1
        i32.shl
        local.set 3
        local.get 0
        local.get 2
        i32.lt_u
        br_if 0 (;@2;)
      end
    end
    block  ;; label = @1
      i32.const -64
      local.get 0
      i32.sub
      local.get 1
      i32.gt_u
      br_if 0 (;@1;)
      i32.const 0
      i32.const 48
      i32.store offset=1059540
      i32.const 0
      return
    end
    block  ;; label = @1
      i32.const 16
      local.get 1
      i32.const 19
      i32.add
      i32.const -16
      i32.and
      local.get 1
      i32.const 11
      i32.lt_u
      select
      local.tee 1
      i32.const 12
      i32.or
      local.get 0
      i32.add
      call $dlmalloc
      local.tee 3
      br_if 0 (;@1;)
      i32.const 0
      return
    end
    local.get 3
    i32.const -8
    i32.add
    local.set 2
    block  ;; label = @1
      block  ;; label = @2
        local.get 0
        i32.const -1
        i32.add
        local.get 3
        i32.and
        br_if 0 (;@2;)
        local.get 2
        local.set 0
        br 1 (;@1;)
      end
      local.get 3
      i32.const -4
      i32.add
      local.tee 4
      i32.load
      local.tee 5
      i32.const -8
      i32.and
      local.get 3
      local.get 0
      i32.add
      i32.const -1
      i32.add
      i32.const 0
      local.get 0
      i32.sub
      i32.and
      i32.const -8
      i32.add
      local.tee 3
      local.get 3
      local.get 0
      i32.add
      local.get 3
      local.get 2
      i32.sub
      i32.const 15
      i32.gt_u
      select
      local.tee 0
      local.get 2
      i32.sub
      local.tee 3
      i32.sub
      local.set 6
      block  ;; label = @2
        local.get 5
        i32.const 3
        i32.and
        br_if 0 (;@2;)
        local.get 0
        local.get 6
        i32.store offset=4
        local.get 0
        local.get 2
        i32.load
        local.get 3
        i32.add
        i32.store
        br 1 (;@1;)
      end
      local.get 0
      local.get 6
      local.get 0
      i32.load offset=4
      i32.const 1
      i32.and
      i32.or
      i32.const 2
      i32.or
      i32.store offset=4
      local.get 0
      local.get 6
      i32.add
      local.tee 6
      local.get 6
      i32.load offset=4
      i32.const 1
      i32.or
      i32.store offset=4
      local.get 4
      local.get 3
      local.get 4
      i32.load
      i32.const 1
      i32.and
      i32.or
      i32.const 2
      i32.or
      i32.store
      local.get 0
      local.get 0
      i32.load offset=4
      i32.const 1
      i32.or
      i32.store offset=4
      local.get 2
      local.get 3
      call $dispose_chunk
    end
    block  ;; label = @1
      local.get 0
      i32.load offset=4
      local.tee 3
      i32.const 3
      i32.and
      i32.eqz
      br_if 0 (;@1;)
      local.get 3
      i32.const -8
      i32.and
      local.tee 2
      local.get 1
      i32.const 16
      i32.add
      i32.le_u
      br_if 0 (;@1;)
      local.get 0
      local.get 1
      local.get 3
      i32.const 1
      i32.and
      i32.or
      i32.const 2
      i32.or
      i32.store offset=4
      local.get 0
      local.get 1
      i32.add
      local.tee 3
      local.get 2
      local.get 1
      i32.sub
      local.tee 1
      i32.const 3
      i32.or
      i32.store offset=4
      local.get 0
      local.get 2
      i32.add
      local.tee 2
      local.get 2
      i32.load offset=4
      i32.const 1
      i32.or
      i32.store offset=4
      local.get 3
      local.get 1
      call $dispose_chunk
    end
    local.get 0
    i32.const 8
    i32.add)
  (func $aligned_alloc (type 2) (param i32 i32) (result i32)
    block  ;; label = @1
      local.get 0
      i32.const 16
      i32.gt_u
      br_if 0 (;@1;)
      local.get 1
      call $dlmalloc
      return
    end
    local.get 0
    local.get 1
    call $internal_memalign)
  (func $sbrk (type 14) (param i32) (result i32)
    block  ;; label = @1
      local.get 0
      br_if 0 (;@1;)
      memory.size
      i32.const 16
      i32.shl
      return
    end
    block  ;; label = @1
      local.get 0
      i32.const 65535
      i32.and
      br_if 0 (;@1;)
      local.get 0
      i32.const -1
      i32.le_s
      br_if 0 (;@1;)
      block  ;; label = @2
        local.get 0
        i32.const 16
        i32.shr_u
        memory.grow
        local.tee 0
        i32.const -1
        i32.ne
        br_if 0 (;@2;)
        i32.const 0
        i32.const 48
        i32.store offset=1059540
        i32.const -1
        return
      end
      local.get 0
      i32.const 16
      i32.shl
      return
    end
    call $abort
    unreachable)
  (func $getenv (type 14) (param i32) (result i32)
    (local i32 i32 i32 i32)
    i32.const 0
    local.set 1
    block  ;; label = @1
      local.get 0
      i32.const 61
      call $__strchrnul
      local.tee 2
      local.get 0
      i32.sub
      local.tee 3
      i32.eqz
      br_if 0 (;@1;)
      local.get 2
      i32.load8_u
      br_if 0 (;@1;)
      i32.const 0
      i32.load offset=1058980
      local.tee 4
      i32.eqz
      br_if 0 (;@1;)
      local.get 4
      i32.load
      local.tee 2
      i32.eqz
      br_if 0 (;@1;)
      local.get 4
      i32.const 4
      i32.add
      local.set 4
      block  ;; label = @2
        loop  ;; label = @3
          block  ;; label = @4
            local.get 0
            local.get 2
            local.get 3
            call $strncmp
            br_if 0 (;@4;)
            local.get 2
            local.get 3
            i32.add
            local.tee 2
            i32.load8_u
            i32.const 61
            i32.eq
            br_if 2 (;@2;)
          end
          local.get 4
          i32.load
          local.set 2
          local.get 4
          i32.const 4
          i32.add
          local.set 4
          local.get 2
          br_if 0 (;@3;)
          br 2 (;@1;)
        end
      end
      local.get 2
      i32.const 1
      i32.add
      local.set 1
    end
    local.get 1)
  (func $strlen (type 14) (param i32) (result i32)
    (local i32 i32 i32)
    local.get 0
    local.set 1
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 0
          i32.const 3
          i32.and
          i32.eqz
          br_if 0 (;@3;)
          block  ;; label = @4
            local.get 0
            i32.load8_u
            br_if 0 (;@4;)
            local.get 0
            local.get 0
            i32.sub
            return
          end
          local.get 0
          i32.const 1
          i32.add
          local.set 1
          loop  ;; label = @4
            local.get 1
            i32.const 3
            i32.and
            i32.eqz
            br_if 1 (;@3;)
            local.get 1
            i32.load8_u
            local.set 2
            local.get 1
            i32.const 1
            i32.add
            local.tee 3
            local.set 1
            local.get 2
            i32.eqz
            br_if 2 (;@2;)
            br 0 (;@4;)
          end
        end
        local.get 1
        i32.const -4
        i32.add
        local.set 1
        loop  ;; label = @3
          local.get 1
          i32.const 4
          i32.add
          local.tee 1
          i32.load
          local.tee 2
          i32.const -1
          i32.xor
          local.get 2
          i32.const -16843009
          i32.add
          i32.and
          i32.const -2139062144
          i32.and
          i32.eqz
          br_if 0 (;@3;)
        end
        block  ;; label = @3
          local.get 2
          i32.const 255
          i32.and
          br_if 0 (;@3;)
          local.get 1
          local.get 0
          i32.sub
          return
        end
        loop  ;; label = @3
          local.get 1
          i32.load8_u offset=1
          local.set 2
          local.get 1
          i32.const 1
          i32.add
          local.tee 3
          local.set 1
          local.get 2
          br_if 0 (;@3;)
          br 2 (;@1;)
        end
      end
      local.get 3
      i32.const -1
      i32.add
      local.set 3
    end
    local.get 3
    local.get 0
    i32.sub)
  (func $strerror (type 14) (param i32) (result i32)
    (local i32 i32 i32 i32)
    i32.const 0
    local.set 1
    block  ;; label = @1
      i32.const 0
      i32.load offset=1059572
      local.tee 2
      br_if 0 (;@1;)
      i32.const 1059548
      local.set 2
      i32.const 0
      i32.const 1059548
      i32.store offset=1059572
    end
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          loop  ;; label = @4
            local.get 1
            i32.const 1052640
            i32.add
            i32.load8_u
            local.get 0
            i32.eq
            br_if 1 (;@3;)
            i32.const 77
            local.set 3
            local.get 1
            i32.const 1
            i32.add
            local.tee 1
            i32.const 77
            i32.ne
            br_if 0 (;@4;)
            br 2 (;@2;)
          end
        end
        local.get 1
        local.set 3
        local.get 1
        br_if 0 (;@2;)
        i32.const 1052720
        local.set 4
        br 1 (;@1;)
      end
      i32.const 1052720
      local.set 1
      loop  ;; label = @2
        local.get 1
        i32.load8_u
        local.set 0
        local.get 1
        i32.const 1
        i32.add
        local.tee 4
        local.set 1
        local.get 0
        br_if 0 (;@2;)
        local.get 4
        local.set 1
        local.get 3
        i32.const -1
        i32.add
        local.tee 3
        br_if 0 (;@2;)
      end
    end
    local.get 4
    local.get 2
    i32.load offset=20
    call $__lctrans)
  (func $strerror_r (type 6) (param i32 i32 i32) (result i32)
    (local i32)
    block  ;; label = @1
      block  ;; label = @2
        local.get 0
        call $strerror
        local.tee 0
        call $strlen
        local.tee 3
        local.get 2
        i32.lt_u
        br_if 0 (;@2;)
        i32.const 68
        local.set 3
        local.get 2
        i32.eqz
        br_if 1 (;@1;)
        local.get 1
        local.get 0
        local.get 2
        i32.const -1
        i32.add
        local.tee 2
        call $memcpy
        local.get 2
        i32.add
        i32.const 0
        i32.store8
        i32.const 68
        return
      end
      local.get 1
      local.get 0
      local.get 3
      i32.const 1
      i32.add
      call $memcpy
      drop
      i32.const 0
      local.set 3
    end
    local.get 3)
  (func $memcpy (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32 i32)
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.eqz
        br_if 0 (;@2;)
        local.get 1
        i32.const 3
        i32.and
        i32.eqz
        br_if 0 (;@2;)
        local.get 0
        local.set 3
        loop  ;; label = @3
          local.get 3
          local.get 1
          i32.load8_u
          i32.store8
          local.get 2
          i32.const -1
          i32.add
          local.set 4
          local.get 3
          i32.const 1
          i32.add
          local.set 3
          local.get 1
          i32.const 1
          i32.add
          local.set 1
          local.get 2
          i32.const 1
          i32.eq
          br_if 2 (;@1;)
          local.get 4
          local.set 2
          local.get 1
          i32.const 3
          i32.and
          br_if 0 (;@3;)
          br 2 (;@1;)
        end
      end
      local.get 2
      local.set 4
      local.get 0
      local.set 3
    end
    block  ;; label = @1
      block  ;; label = @2
        local.get 3
        i32.const 3
        i32.and
        local.tee 2
        br_if 0 (;@2;)
        block  ;; label = @3
          block  ;; label = @4
            local.get 4
            i32.const 16
            i32.ge_u
            br_if 0 (;@4;)
            local.get 4
            local.set 2
            br 1 (;@3;)
          end
          local.get 4
          i32.const -16
          i32.add
          local.set 2
          loop  ;; label = @4
            local.get 3
            local.get 1
            i32.load
            i32.store
            local.get 3
            i32.const 4
            i32.add
            local.get 1
            i32.const 4
            i32.add
            i32.load
            i32.store
            local.get 3
            i32.const 8
            i32.add
            local.get 1
            i32.const 8
            i32.add
            i32.load
            i32.store
            local.get 3
            i32.const 12
            i32.add
            local.get 1
            i32.const 12
            i32.add
            i32.load
            i32.store
            local.get 3
            i32.const 16
            i32.add
            local.set 3
            local.get 1
            i32.const 16
            i32.add
            local.set 1
            local.get 4
            i32.const -16
            i32.add
            local.tee 4
            i32.const 15
            i32.gt_u
            br_if 0 (;@4;)
          end
        end
        block  ;; label = @3
          local.get 2
          i32.const 8
          i32.and
          i32.eqz
          br_if 0 (;@3;)
          local.get 3
          local.get 1
          i64.load align=4
          i64.store align=4
          local.get 1
          i32.const 8
          i32.add
          local.set 1
          local.get 3
          i32.const 8
          i32.add
          local.set 3
        end
        block  ;; label = @3
          local.get 2
          i32.const 4
          i32.and
          i32.eqz
          br_if 0 (;@3;)
          local.get 3
          local.get 1
          i32.load
          i32.store
          local.get 1
          i32.const 4
          i32.add
          local.set 1
          local.get 3
          i32.const 4
          i32.add
          local.set 3
        end
        block  ;; label = @3
          local.get 2
          i32.const 2
          i32.and
          i32.eqz
          br_if 0 (;@3;)
          local.get 3
          local.get 1
          i32.load8_u
          i32.store8
          local.get 3
          local.get 1
          i32.load8_u offset=1
          i32.store8 offset=1
          local.get 3
          i32.const 2
          i32.add
          local.set 3
          local.get 1
          i32.const 2
          i32.add
          local.set 1
        end
        local.get 2
        i32.const 1
        i32.and
        i32.eqz
        br_if 1 (;@1;)
        local.get 3
        local.get 1
        i32.load8_u
        i32.store8
        local.get 0
        return
      end
      block  ;; label = @2
        local.get 4
        i32.const 32
        i32.lt_u
        br_if 0 (;@2;)
        local.get 2
        i32.const -1
        i32.add
        local.tee 2
        i32.const 2
        i32.gt_u
        br_if 0 (;@2;)
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              local.get 2
              br_table 0 (;@5;) 1 (;@4;) 2 (;@3;) 0 (;@5;)
            end
            local.get 3
            local.get 1
            i32.load8_u offset=1
            i32.store8 offset=1
            local.get 3
            local.get 1
            i32.load
            local.tee 5
            i32.store8
            local.get 3
            local.get 1
            i32.load8_u offset=2
            i32.store8 offset=2
            local.get 4
            i32.const -3
            i32.add
            local.set 6
            local.get 3
            i32.const 3
            i32.add
            local.set 7
            local.get 4
            i32.const -20
            i32.add
            i32.const -16
            i32.and
            local.set 8
            i32.const 0
            local.set 2
            loop  ;; label = @5
              local.get 7
              local.get 2
              i32.add
              local.tee 3
              local.get 1
              local.get 2
              i32.add
              local.tee 9
              i32.const 4
              i32.add
              i32.load
              local.tee 10
              i32.const 8
              i32.shl
              local.get 5
              i32.const 24
              i32.shr_u
              i32.or
              i32.store
              local.get 3
              i32.const 4
              i32.add
              local.get 9
              i32.const 8
              i32.add
              i32.load
              local.tee 5
              i32.const 8
              i32.shl
              local.get 10
              i32.const 24
              i32.shr_u
              i32.or
              i32.store
              local.get 3
              i32.const 8
              i32.add
              local.get 9
              i32.const 12
              i32.add
              i32.load
              local.tee 10
              i32.const 8
              i32.shl
              local.get 5
              i32.const 24
              i32.shr_u
              i32.or
              i32.store
              local.get 3
              i32.const 12
              i32.add
              local.get 9
              i32.const 16
              i32.add
              i32.load
              local.tee 5
              i32.const 8
              i32.shl
              local.get 10
              i32.const 24
              i32.shr_u
              i32.or
              i32.store
              local.get 2
              i32.const 16
              i32.add
              local.set 2
              local.get 6
              i32.const -16
              i32.add
              local.tee 6
              i32.const 16
              i32.gt_u
              br_if 0 (;@5;)
            end
            local.get 7
            local.get 2
            i32.add
            local.set 3
            local.get 1
            local.get 2
            i32.add
            i32.const 3
            i32.add
            local.set 1
            local.get 4
            local.get 8
            i32.sub
            i32.const -19
            i32.add
            local.set 4
            br 2 (;@2;)
          end
          local.get 3
          local.get 1
          i32.load
          local.tee 5
          i32.store8
          local.get 3
          local.get 1
          i32.load8_u offset=1
          i32.store8 offset=1
          local.get 4
          i32.const -2
          i32.add
          local.set 6
          local.get 3
          i32.const 2
          i32.add
          local.set 7
          local.get 4
          i32.const -20
          i32.add
          i32.const -16
          i32.and
          local.set 8
          i32.const 0
          local.set 2
          loop  ;; label = @4
            local.get 7
            local.get 2
            i32.add
            local.tee 3
            local.get 1
            local.get 2
            i32.add
            local.tee 9
            i32.const 4
            i32.add
            i32.load
            local.tee 10
            i32.const 16
            i32.shl
            local.get 5
            i32.const 16
            i32.shr_u
            i32.or
            i32.store
            local.get 3
            i32.const 4
            i32.add
            local.get 9
            i32.const 8
            i32.add
            i32.load
            local.tee 5
            i32.const 16
            i32.shl
            local.get 10
            i32.const 16
            i32.shr_u
            i32.or
            i32.store
            local.get 3
            i32.const 8
            i32.add
            local.get 9
            i32.const 12
            i32.add
            i32.load
            local.tee 10
            i32.const 16
            i32.shl
            local.get 5
            i32.const 16
            i32.shr_u
            i32.or
            i32.store
            local.get 3
            i32.const 12
            i32.add
            local.get 9
            i32.const 16
            i32.add
            i32.load
            local.tee 5
            i32.const 16
            i32.shl
            local.get 10
            i32.const 16
            i32.shr_u
            i32.or
            i32.store
            local.get 2
            i32.const 16
            i32.add
            local.set 2
            local.get 6
            i32.const -16
            i32.add
            local.tee 6
            i32.const 17
            i32.gt_u
            br_if 0 (;@4;)
          end
          local.get 7
          local.get 2
          i32.add
          local.set 3
          local.get 1
          local.get 2
          i32.add
          i32.const 2
          i32.add
          local.set 1
          local.get 4
          local.get 8
          i32.sub
          i32.const -18
          i32.add
          local.set 4
          br 1 (;@2;)
        end
        local.get 3
        local.get 1
        i32.load
        local.tee 5
        i32.store8
        local.get 4
        i32.const -1
        i32.add
        local.set 6
        local.get 3
        i32.const 1
        i32.add
        local.set 7
        local.get 4
        i32.const -20
        i32.add
        i32.const -16
        i32.and
        local.set 8
        i32.const 0
        local.set 2
        loop  ;; label = @3
          local.get 7
          local.get 2
          i32.add
          local.tee 3
          local.get 1
          local.get 2
          i32.add
          local.tee 9
          i32.const 4
          i32.add
          i32.load
          local.tee 10
          i32.const 24
          i32.shl
          local.get 5
          i32.const 8
          i32.shr_u
          i32.or
          i32.store
          local.get 3
          i32.const 4
          i32.add
          local.get 9
          i32.const 8
          i32.add
          i32.load
          local.tee 5
          i32.const 24
          i32.shl
          local.get 10
          i32.const 8
          i32.shr_u
          i32.or
          i32.store
          local.get 3
          i32.const 8
          i32.add
          local.get 9
          i32.const 12
          i32.add
          i32.load
          local.tee 10
          i32.const 24
          i32.shl
          local.get 5
          i32.const 8
          i32.shr_u
          i32.or
          i32.store
          local.get 3
          i32.const 12
          i32.add
          local.get 9
          i32.const 16
          i32.add
          i32.load
          local.tee 5
          i32.const 24
          i32.shl
          local.get 10
          i32.const 8
          i32.shr_u
          i32.or
          i32.store
          local.get 2
          i32.const 16
          i32.add
          local.set 2
          local.get 6
          i32.const -16
          i32.add
          local.tee 6
          i32.const 18
          i32.gt_u
          br_if 0 (;@3;)
        end
        local.get 7
        local.get 2
        i32.add
        local.set 3
        local.get 1
        local.get 2
        i32.add
        i32.const 1
        i32.add
        local.set 1
        local.get 4
        local.get 8
        i32.sub
        i32.const -17
        i32.add
        local.set 4
      end
      block  ;; label = @2
        local.get 4
        i32.const 16
        i32.and
        i32.eqz
        br_if 0 (;@2;)
        local.get 3
        local.get 1
        i32.load16_u align=1
        i32.store16 align=1
        local.get 3
        local.get 1
        i32.load8_u offset=2
        i32.store8 offset=2
        local.get 3
        local.get 1
        i32.load8_u offset=3
        i32.store8 offset=3
        local.get 3
        local.get 1
        i32.load8_u offset=4
        i32.store8 offset=4
        local.get 3
        local.get 1
        i32.load8_u offset=5
        i32.store8 offset=5
        local.get 3
        local.get 1
        i32.load8_u offset=6
        i32.store8 offset=6
        local.get 3
        local.get 1
        i32.load8_u offset=7
        i32.store8 offset=7
        local.get 3
        local.get 1
        i32.load8_u offset=8
        i32.store8 offset=8
        local.get 3
        local.get 1
        i32.load8_u offset=9
        i32.store8 offset=9
        local.get 3
        local.get 1
        i32.load8_u offset=10
        i32.store8 offset=10
        local.get 3
        local.get 1
        i32.load8_u offset=11
        i32.store8 offset=11
        local.get 3
        local.get 1
        i32.load8_u offset=12
        i32.store8 offset=12
        local.get 3
        local.get 1
        i32.load8_u offset=13
        i32.store8 offset=13
        local.get 3
        local.get 1
        i32.load8_u offset=14
        i32.store8 offset=14
        local.get 3
        local.get 1
        i32.load8_u offset=15
        i32.store8 offset=15
        local.get 3
        i32.const 16
        i32.add
        local.set 3
        local.get 1
        i32.const 16
        i32.add
        local.set 1
      end
      block  ;; label = @2
        local.get 4
        i32.const 8
        i32.and
        i32.eqz
        br_if 0 (;@2;)
        local.get 3
        local.get 1
        i32.load8_u
        i32.store8
        local.get 3
        local.get 1
        i32.load8_u offset=1
        i32.store8 offset=1
        local.get 3
        local.get 1
        i32.load8_u offset=2
        i32.store8 offset=2
        local.get 3
        local.get 1
        i32.load8_u offset=3
        i32.store8 offset=3
        local.get 3
        local.get 1
        i32.load8_u offset=4
        i32.store8 offset=4
        local.get 3
        local.get 1
        i32.load8_u offset=5
        i32.store8 offset=5
        local.get 3
        local.get 1
        i32.load8_u offset=6
        i32.store8 offset=6
        local.get 3
        local.get 1
        i32.load8_u offset=7
        i32.store8 offset=7
        local.get 3
        i32.const 8
        i32.add
        local.set 3
        local.get 1
        i32.const 8
        i32.add
        local.set 1
      end
      block  ;; label = @2
        local.get 4
        i32.const 4
        i32.and
        i32.eqz
        br_if 0 (;@2;)
        local.get 3
        local.get 1
        i32.load8_u
        i32.store8
        local.get 3
        local.get 1
        i32.load8_u offset=1
        i32.store8 offset=1
        local.get 3
        local.get 1
        i32.load8_u offset=2
        i32.store8 offset=2
        local.get 3
        local.get 1
        i32.load8_u offset=3
        i32.store8 offset=3
        local.get 3
        i32.const 4
        i32.add
        local.set 3
        local.get 1
        i32.const 4
        i32.add
        local.set 1
      end
      block  ;; label = @2
        local.get 4
        i32.const 2
        i32.and
        i32.eqz
        br_if 0 (;@2;)
        local.get 3
        local.get 1
        i32.load8_u
        i32.store8
        local.get 3
        local.get 1
        i32.load8_u offset=1
        i32.store8 offset=1
        local.get 3
        i32.const 2
        i32.add
        local.set 3
        local.get 1
        i32.const 2
        i32.add
        local.set 1
      end
      local.get 4
      i32.const 1
      i32.and
      i32.eqz
      br_if 0 (;@1;)
      local.get 3
      local.get 1
      i32.load8_u
      i32.store8
    end
    local.get 0)
  (func $__strchrnul (type 2) (param i32 i32) (result i32)
    (local i32 i32)
    block  ;; label = @1
      local.get 1
      i32.const 255
      i32.and
      local.tee 2
      i32.eqz
      br_if 0 (;@1;)
      block  ;; label = @2
        block  ;; label = @3
          local.get 0
          i32.const 3
          i32.and
          i32.eqz
          br_if 0 (;@3;)
          loop  ;; label = @4
            local.get 0
            i32.load8_u
            local.tee 3
            i32.eqz
            br_if 2 (;@2;)
            local.get 3
            local.get 1
            i32.const 255
            i32.and
            i32.eq
            br_if 2 (;@2;)
            local.get 0
            i32.const 1
            i32.add
            local.tee 0
            i32.const 3
            i32.and
            br_if 0 (;@4;)
          end
        end
        block  ;; label = @3
          local.get 0
          i32.load
          local.tee 3
          i32.const -1
          i32.xor
          local.get 3
          i32.const -16843009
          i32.add
          i32.and
          i32.const -2139062144
          i32.and
          br_if 0 (;@3;)
          local.get 2
          i32.const 16843009
          i32.mul
          local.set 2
          loop  ;; label = @4
            local.get 3
            local.get 2
            i32.xor
            local.tee 3
            i32.const -1
            i32.xor
            local.get 3
            i32.const -16843009
            i32.add
            i32.and
            i32.const -2139062144
            i32.and
            br_if 1 (;@3;)
            local.get 0
            i32.load offset=4
            local.set 3
            local.get 0
            i32.const 4
            i32.add
            local.set 0
            local.get 3
            i32.const -1
            i32.xor
            local.get 3
            i32.const -16843009
            i32.add
            i32.and
            i32.const -2139062144
            i32.and
            i32.eqz
            br_if 0 (;@4;)
          end
        end
        local.get 0
        i32.const -1
        i32.add
        local.set 0
        loop  ;; label = @3
          local.get 0
          i32.const 1
          i32.add
          local.tee 0
          i32.load8_u
          local.tee 3
          i32.eqz
          br_if 1 (;@2;)
          local.get 3
          local.get 1
          i32.const 255
          i32.and
          i32.ne
          br_if 0 (;@3;)
        end
      end
      local.get 0
      return
    end
    local.get 0
    local.get 0
    call $strlen
    i32.add)
  (func $memset (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32 i64)
    block  ;; label = @1
      local.get 2
      i32.eqz
      br_if 0 (;@1;)
      local.get 0
      local.get 1
      i32.store8
      local.get 2
      local.get 0
      i32.add
      local.tee 3
      i32.const -1
      i32.add
      local.get 1
      i32.store8
      local.get 2
      i32.const 3
      i32.lt_u
      br_if 0 (;@1;)
      local.get 0
      local.get 1
      i32.store8 offset=2
      local.get 0
      local.get 1
      i32.store8 offset=1
      local.get 3
      i32.const -3
      i32.add
      local.get 1
      i32.store8
      local.get 3
      i32.const -2
      i32.add
      local.get 1
      i32.store8
      local.get 2
      i32.const 7
      i32.lt_u
      br_if 0 (;@1;)
      local.get 0
      local.get 1
      i32.store8 offset=3
      local.get 3
      i32.const -4
      i32.add
      local.get 1
      i32.store8
      local.get 2
      i32.const 9
      i32.lt_u
      br_if 0 (;@1;)
      local.get 0
      i32.const 0
      local.get 0
      i32.sub
      i32.const 3
      i32.and
      local.tee 4
      i32.add
      local.tee 3
      local.get 1
      i32.const 255
      i32.and
      i32.const 16843009
      i32.mul
      local.tee 1
      i32.store
      local.get 3
      local.get 2
      local.get 4
      i32.sub
      i32.const -4
      i32.and
      local.tee 4
      i32.add
      local.tee 2
      i32.const -4
      i32.add
      local.get 1
      i32.store
      local.get 4
      i32.const 9
      i32.lt_u
      br_if 0 (;@1;)
      local.get 3
      local.get 1
      i32.store offset=8
      local.get 3
      local.get 1
      i32.store offset=4
      local.get 2
      i32.const -8
      i32.add
      local.get 1
      i32.store
      local.get 2
      i32.const -12
      i32.add
      local.get 1
      i32.store
      local.get 4
      i32.const 25
      i32.lt_u
      br_if 0 (;@1;)
      local.get 3
      local.get 1
      i32.store offset=24
      local.get 3
      local.get 1
      i32.store offset=20
      local.get 3
      local.get 1
      i32.store offset=16
      local.get 3
      local.get 1
      i32.store offset=12
      local.get 2
      i32.const -16
      i32.add
      local.get 1
      i32.store
      local.get 2
      i32.const -20
      i32.add
      local.get 1
      i32.store
      local.get 2
      i32.const -24
      i32.add
      local.get 1
      i32.store
      local.get 2
      i32.const -28
      i32.add
      local.get 1
      i32.store
      local.get 4
      local.get 3
      i32.const 4
      i32.and
      i32.const 24
      i32.or
      local.tee 5
      i32.sub
      local.tee 2
      i32.const 32
      i32.lt_u
      br_if 0 (;@1;)
      local.get 1
      i64.extend_i32_u
      local.tee 6
      i64.const 32
      i64.shl
      local.get 6
      i64.or
      local.set 6
      local.get 3
      local.get 5
      i32.add
      local.set 1
      loop  ;; label = @2
        local.get 1
        local.get 6
        i64.store
        local.get 1
        i32.const 24
        i32.add
        local.get 6
        i64.store
        local.get 1
        i32.const 16
        i32.add
        local.get 6
        i64.store
        local.get 1
        i32.const 8
        i32.add
        local.get 6
        i64.store
        local.get 1
        i32.const 32
        i32.add
        local.set 1
        local.get 2
        i32.const -32
        i32.add
        local.tee 2
        i32.const 31
        i32.gt_u
        br_if 0 (;@2;)
      end
    end
    local.get 0)
  (func $strncmp (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32)
    block  ;; label = @1
      local.get 2
      br_if 0 (;@1;)
      i32.const 0
      return
    end
    i32.const 0
    local.set 3
    block  ;; label = @1
      local.get 0
      i32.load8_u
      local.tee 4
      i32.eqz
      br_if 0 (;@1;)
      local.get 0
      i32.const 1
      i32.add
      local.set 0
      local.get 2
      i32.const -1
      i32.add
      local.set 2
      loop  ;; label = @2
        block  ;; label = @3
          local.get 4
          i32.const 255
          i32.and
          local.get 1
          i32.load8_u
          local.tee 5
          i32.eq
          br_if 0 (;@3;)
          local.get 4
          local.set 3
          br 2 (;@1;)
        end
        block  ;; label = @3
          local.get 2
          br_if 0 (;@3;)
          local.get 4
          local.set 3
          br 2 (;@1;)
        end
        block  ;; label = @3
          local.get 5
          br_if 0 (;@3;)
          local.get 4
          local.set 3
          br 2 (;@1;)
        end
        local.get 2
        i32.const -1
        i32.add
        local.set 2
        local.get 1
        i32.const 1
        i32.add
        local.set 1
        local.get 0
        i32.load8_u
        local.set 4
        local.get 0
        i32.const 1
        i32.add
        local.set 0
        local.get 4
        br_if 0 (;@2;)
      end
    end
    local.get 3
    i32.const 255
    i32.and
    local.get 1
    i32.load8_u
    i32.sub)
  (func $memcmp (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32)
    i32.const 0
    local.set 3
    block  ;; label = @1
      local.get 2
      i32.eqz
      br_if 0 (;@1;)
      block  ;; label = @2
        loop  ;; label = @3
          local.get 0
          i32.load8_u
          local.tee 4
          local.get 1
          i32.load8_u
          local.tee 5
          i32.ne
          br_if 1 (;@2;)
          local.get 1
          i32.const 1
          i32.add
          local.set 1
          local.get 0
          i32.const 1
          i32.add
          local.set 0
          local.get 2
          i32.const -1
          i32.add
          local.tee 2
          br_if 0 (;@3;)
          br 2 (;@1;)
        end
      end
      local.get 4
      local.get 5
      i32.sub
      local.set 3
    end
    local.get 3)
  (func $dummy (type 2) (param i32 i32) (result i32)
    local.get 0)
  (func $__lctrans (type 2) (param i32 i32) (result i32)
    local.get 0
    local.get 1
    call $dummy)
  (func $_ZN5alloc5alloc18handle_alloc_error17hdb3c7feb2edf717fE (type 4) (param i32 i32)
    local.get 0
    local.get 1
    call $rust_oom
    unreachable)
  (func $_ZN5alloc7raw_vec17capacity_overflow17h60fd539dfca5134dE (type 9)
    i32.const 1054317
    i32.const 17
    i32.const 1054336
    call $_ZN4core9panicking5panic17he9463ceb3e2615beE
    unreachable)
  (func $_ZN5alloc6string104_$LT$impl$u20$core..convert..From$LT$alloc..string..String$GT$$u20$for$u20$alloc..vec..Vec$LT$u8$GT$$GT$4from17hdf1aaa94a1e2e337E (type 4) (param i32 i32)
    local.get 0
    local.get 1
    i64.load align=4
    i64.store align=4
    local.get 0
    i32.const 8
    i32.add
    local.get 1
    i32.const 8
    i32.add
    i32.load
    i32.store)
  (func $_ZN4core3ptr13drop_in_place17h01d2cdf0847f87a8E (type 0) (param i32))
  (func $_ZN4core9panicking18panic_bounds_check17h8c4f03235e0b5d9bE (type 5) (param i32 i32 i32)
    (local i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    local.get 2
    i32.store offset=4
    local.get 3
    local.get 1
    i32.store
    local.get 3
    i32.const 28
    i32.add
    i32.const 2
    i32.store
    local.get 3
    i32.const 44
    i32.add
    i32.const 18
    i32.store
    local.get 3
    i64.const 2
    i64.store offset=12 align=4
    local.get 3
    i32.const 1054548
    i32.store offset=8
    local.get 3
    i32.const 18
    i32.store offset=36
    local.get 3
    local.get 3
    i32.const 32
    i32.add
    i32.store offset=24
    local.get 3
    local.get 3
    i32.store offset=40
    local.get 3
    local.get 3
    i32.const 4
    i32.add
    i32.store offset=32
    local.get 3
    i32.const 8
    i32.add
    local.get 0
    call $_ZN4core9panicking9panic_fmt17h98142caac1112f39E
    unreachable)
  (func $_ZN4core9panicking5panic17he9463ceb3e2615beE (type 5) (param i32 i32 i32)
    (local i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    i32.const 20
    i32.add
    i32.const 0
    i32.store
    local.get 3
    i32.const 1054352
    i32.store offset=16
    local.get 3
    i64.const 1
    i64.store offset=4 align=4
    local.get 3
    local.get 1
    i32.store offset=28
    local.get 3
    local.get 0
    i32.store offset=24
    local.get 3
    local.get 3
    i32.const 24
    i32.add
    i32.store
    local.get 3
    local.get 2
    call $_ZN4core9panicking9panic_fmt17h98142caac1112f39E
    unreachable)
  (func $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E (type 4) (param i32 i32)
    (local i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 1
    i32.store offset=4
    local.get 2
    local.get 0
    i32.store
    local.get 2
    i32.const 28
    i32.add
    i32.const 2
    i32.store
    local.get 2
    i32.const 44
    i32.add
    i32.const 18
    i32.store
    local.get 2
    i64.const 2
    i64.store offset=12 align=4
    local.get 2
    i32.const 1054976
    i32.store offset=8
    local.get 2
    i32.const 18
    i32.store offset=36
    local.get 2
    local.get 2
    i32.const 32
    i32.add
    i32.store offset=24
    local.get 2
    local.get 2
    i32.const 4
    i32.add
    i32.store offset=40
    local.get 2
    local.get 2
    i32.store offset=32
    local.get 2
    i32.const 8
    i32.add
    i32.const 1054992
    call $_ZN4core9panicking9panic_fmt17h98142caac1112f39E
    unreachable)
  (func $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE (type 4) (param i32 i32)
    (local i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 1
    i32.store offset=4
    local.get 2
    local.get 0
    i32.store
    local.get 2
    i32.const 28
    i32.add
    i32.const 2
    i32.store
    local.get 2
    i32.const 44
    i32.add
    i32.const 18
    i32.store
    local.get 2
    i64.const 2
    i64.store offset=12 align=4
    local.get 2
    i32.const 1055044
    i32.store offset=8
    local.get 2
    i32.const 18
    i32.store offset=36
    local.get 2
    local.get 2
    i32.const 32
    i32.add
    i32.store offset=24
    local.get 2
    local.get 2
    i32.const 4
    i32.add
    i32.store offset=40
    local.get 2
    local.get 2
    i32.store offset=32
    local.get 2
    i32.const 8
    i32.add
    i32.const 1055060
    call $_ZN4core9panicking9panic_fmt17h98142caac1112f39E
    unreachable)
  (func $_ZN4core3fmt9Formatter3pad17h7b301e85900e29c6E (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)
    local.get 0
    i32.load offset=16
    local.set 3
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 0
            i32.load offset=8
            local.tee 4
            i32.const 1
            i32.eq
            br_if 0 (;@4;)
            local.get 3
            br_if 1 (;@3;)
            local.get 0
            i32.load offset=24
            local.get 1
            local.get 2
            local.get 0
            i32.const 28
            i32.add
            i32.load
            i32.load offset=12
            call_indirect (type 6)
            local.set 3
            br 3 (;@1;)
          end
          local.get 3
          i32.eqz
          br_if 1 (;@2;)
        end
        block  ;; label = @3
          block  ;; label = @4
            local.get 2
            br_if 0 (;@4;)
            i32.const 0
            local.set 2
            br 1 (;@3;)
          end
          local.get 1
          local.get 2
          i32.add
          local.set 5
          local.get 0
          i32.const 20
          i32.add
          i32.load
          i32.const 1
          i32.add
          local.set 6
          i32.const 0
          local.set 7
          local.get 1
          local.set 3
          local.get 1
          local.set 8
          loop  ;; label = @4
            local.get 3
            i32.const 1
            i32.add
            local.set 9
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  local.get 3
                  i32.load8_s
                  local.tee 10
                  i32.const -1
                  i32.gt_s
                  br_if 0 (;@7;)
                  block  ;; label = @8
                    block  ;; label = @9
                      local.get 9
                      local.get 5
                      i32.ne
                      br_if 0 (;@9;)
                      i32.const 0
                      local.set 11
                      local.get 5
                      local.set 3
                      br 1 (;@8;)
                    end
                    local.get 3
                    i32.load8_u offset=1
                    i32.const 63
                    i32.and
                    local.set 11
                    local.get 3
                    i32.const 2
                    i32.add
                    local.tee 9
                    local.set 3
                  end
                  local.get 10
                  i32.const 31
                  i32.and
                  local.set 12
                  block  ;; label = @8
                    local.get 10
                    i32.const 255
                    i32.and
                    local.tee 10
                    i32.const 223
                    i32.gt_u
                    br_if 0 (;@8;)
                    local.get 11
                    local.get 12
                    i32.const 6
                    i32.shl
                    i32.or
                    local.set 10
                    br 2 (;@6;)
                  end
                  block  ;; label = @8
                    block  ;; label = @9
                      local.get 3
                      local.get 5
                      i32.ne
                      br_if 0 (;@9;)
                      i32.const 0
                      local.set 13
                      local.get 5
                      local.set 14
                      br 1 (;@8;)
                    end
                    local.get 3
                    i32.load8_u
                    i32.const 63
                    i32.and
                    local.set 13
                    local.get 3
                    i32.const 1
                    i32.add
                    local.tee 9
                    local.set 14
                  end
                  local.get 13
                  local.get 11
                  i32.const 6
                  i32.shl
                  i32.or
                  local.set 11
                  block  ;; label = @8
                    local.get 10
                    i32.const 240
                    i32.ge_u
                    br_if 0 (;@8;)
                    local.get 11
                    local.get 12
                    i32.const 12
                    i32.shl
                    i32.or
                    local.set 10
                    br 2 (;@6;)
                  end
                  block  ;; label = @8
                    block  ;; label = @9
                      local.get 14
                      local.get 5
                      i32.ne
                      br_if 0 (;@9;)
                      i32.const 0
                      local.set 10
                      local.get 9
                      local.set 3
                      br 1 (;@8;)
                    end
                    local.get 14
                    i32.const 1
                    i32.add
                    local.set 3
                    local.get 14
                    i32.load8_u
                    i32.const 63
                    i32.and
                    local.set 10
                  end
                  local.get 11
                  i32.const 6
                  i32.shl
                  local.get 12
                  i32.const 18
                  i32.shl
                  i32.const 1835008
                  i32.and
                  i32.or
                  local.get 10
                  i32.or
                  local.tee 10
                  i32.const 1114112
                  i32.ne
                  br_if 2 (;@5;)
                  br 4 (;@3;)
                end
                local.get 10
                i32.const 255
                i32.and
                local.set 10
              end
              local.get 9
              local.set 3
            end
            block  ;; label = @5
              local.get 6
              i32.const -1
              i32.add
              local.tee 6
              i32.eqz
              br_if 0 (;@5;)
              local.get 7
              local.get 8
              i32.sub
              local.get 3
              i32.add
              local.set 7
              local.get 3
              local.set 8
              local.get 5
              local.get 3
              i32.ne
              br_if 1 (;@4;)
              br 2 (;@3;)
            end
          end
          local.get 10
          i32.const 1114112
          i32.eq
          br_if 0 (;@3;)
          block  ;; label = @4
            block  ;; label = @5
              local.get 7
              i32.eqz
              br_if 0 (;@5;)
              local.get 7
              local.get 2
              i32.eq
              br_if 0 (;@5;)
              i32.const 0
              local.set 3
              local.get 7
              local.get 2
              i32.ge_u
              br_if 1 (;@4;)
              local.get 1
              local.get 7
              i32.add
              i32.load8_s
              i32.const -64
              i32.lt_s
              br_if 1 (;@4;)
            end
            local.get 1
            local.set 3
          end
          local.get 7
          local.get 2
          local.get 3
          select
          local.set 2
          local.get 3
          local.get 1
          local.get 3
          select
          local.set 1
        end
        local.get 4
        br_if 0 (;@2;)
        local.get 0
        i32.load offset=24
        local.get 1
        local.get 2
        local.get 0
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        return
      end
      i32.const 0
      local.set 9
      block  ;; label = @2
        local.get 2
        i32.eqz
        br_if 0 (;@2;)
        local.get 2
        local.set 10
        local.get 1
        local.set 3
        loop  ;; label = @3
          local.get 9
          local.get 3
          i32.load8_u
          i32.const 192
          i32.and
          i32.const 128
          i32.eq
          i32.add
          local.set 9
          local.get 3
          i32.const 1
          i32.add
          local.set 3
          local.get 10
          i32.const -1
          i32.add
          local.tee 10
          br_if 0 (;@3;)
        end
      end
      block  ;; label = @2
        local.get 2
        local.get 9
        i32.sub
        local.get 0
        i32.load offset=12
        local.tee 6
        i32.lt_u
        br_if 0 (;@2;)
        local.get 0
        i32.load offset=24
        local.get 1
        local.get 2
        local.get 0
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        return
      end
      i32.const 0
      local.set 7
      i32.const 0
      local.set 9
      block  ;; label = @2
        local.get 2
        i32.eqz
        br_if 0 (;@2;)
        i32.const 0
        local.set 9
        local.get 2
        local.set 10
        local.get 1
        local.set 3
        loop  ;; label = @3
          local.get 9
          local.get 3
          i32.load8_u
          i32.const 192
          i32.and
          i32.const 128
          i32.eq
          i32.add
          local.set 9
          local.get 3
          i32.const 1
          i32.add
          local.set 3
          local.get 10
          i32.const -1
          i32.add
          local.tee 10
          br_if 0 (;@3;)
        end
      end
      local.get 9
      local.get 2
      i32.sub
      local.get 6
      i32.add
      local.tee 9
      local.set 10
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            i32.const 0
            local.get 0
            i32.load8_u offset=32
            local.tee 3
            local.get 3
            i32.const 3
            i32.eq
            select
            br_table 2 (;@2;) 1 (;@3;) 0 (;@4;) 1 (;@3;) 2 (;@2;)
          end
          local.get 9
          i32.const 1
          i32.shr_u
          local.set 7
          local.get 9
          i32.const 1
          i32.add
          i32.const 1
          i32.shr_u
          local.set 10
          br 1 (;@2;)
        end
        i32.const 0
        local.set 10
        local.get 9
        local.set 7
      end
      local.get 7
      i32.const 1
      i32.add
      local.set 3
      block  ;; label = @2
        loop  ;; label = @3
          local.get 3
          i32.const -1
          i32.add
          local.tee 3
          i32.eqz
          br_if 1 (;@2;)
          local.get 0
          i32.load offset=24
          local.get 0
          i32.load offset=4
          local.get 0
          i32.load offset=28
          i32.load offset=16
          call_indirect (type 2)
          i32.eqz
          br_if 0 (;@3;)
        end
        i32.const 1
        return
      end
      local.get 0
      i32.load offset=4
      local.set 9
      i32.const 1
      local.set 3
      local.get 0
      i32.load offset=24
      local.get 1
      local.get 2
      local.get 0
      i32.load offset=28
      i32.load offset=12
      call_indirect (type 6)
      br_if 0 (;@1;)
      local.get 10
      i32.const 1
      i32.add
      local.set 3
      local.get 0
      i32.load offset=28
      local.set 10
      local.get 0
      i32.load offset=24
      local.set 0
      loop  ;; label = @2
        block  ;; label = @3
          local.get 3
          i32.const -1
          i32.add
          local.tee 3
          br_if 0 (;@3;)
          i32.const 0
          return
        end
        local.get 0
        local.get 9
        local.get 10
        i32.load offset=16
        call_indirect (type 2)
        i32.eqz
        br_if 0 (;@2;)
      end
      i32.const 1
      return
    end
    local.get 3)
  (func $_ZN4core3str16slice_error_fail17ha06f3354b25aeac4E (type 3) (param i32 i32 i32 i32)
    (local i32 i32 i32 i32 i32 i32)
    global.get 0
    i32.const 112
    i32.sub
    local.tee 4
    global.set 0
    local.get 4
    local.get 3
    i32.store offset=12
    local.get 4
    local.get 2
    i32.store offset=8
    i32.const 1
    local.set 5
    local.get 1
    local.set 6
    block  ;; label = @1
      local.get 1
      i32.const 257
      i32.lt_u
      br_if 0 (;@1;)
      i32.const 0
      local.get 1
      i32.sub
      local.set 7
      i32.const 256
      local.set 8
      loop  ;; label = @2
        block  ;; label = @3
          local.get 8
          local.get 1
          i32.ge_u
          br_if 0 (;@3;)
          local.get 0
          local.get 8
          i32.add
          i32.load8_s
          i32.const -65
          i32.le_s
          br_if 0 (;@3;)
          i32.const 0
          local.set 5
          local.get 8
          local.set 6
          br 2 (;@1;)
        end
        local.get 8
        i32.const -1
        i32.add
        local.set 6
        i32.const 0
        local.set 5
        local.get 8
        i32.const 1
        i32.eq
        br_if 1 (;@1;)
        local.get 7
        local.get 8
        i32.add
        local.set 9
        local.get 6
        local.set 8
        local.get 9
        i32.const 1
        i32.ne
        br_if 0 (;@2;)
      end
    end
    local.get 4
    local.get 6
    i32.store offset=20
    local.get 4
    local.get 0
    i32.store offset=16
    local.get 4
    i32.const 0
    i32.const 5
    local.get 5
    select
    i32.store offset=28
    local.get 4
    i32.const 1054352
    i32.const 1055430
    local.get 5
    select
    i32.store offset=24
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 2
            local.get 1
            i32.gt_u
            local.tee 8
            br_if 0 (;@4;)
            local.get 3
            local.get 1
            i32.gt_u
            br_if 0 (;@4;)
            local.get 2
            local.get 3
            i32.gt_u
            br_if 1 (;@3;)
            block  ;; label = @5
              block  ;; label = @6
                local.get 2
                i32.eqz
                br_if 0 (;@6;)
                local.get 1
                local.get 2
                i32.eq
                br_if 0 (;@6;)
                local.get 1
                local.get 2
                i32.le_u
                br_if 1 (;@5;)
                local.get 0
                local.get 2
                i32.add
                i32.load8_s
                i32.const -64
                i32.lt_s
                br_if 1 (;@5;)
              end
              local.get 3
              local.set 2
            end
            local.get 4
            local.get 2
            i32.store offset=32
            local.get 2
            i32.eqz
            br_if 2 (;@2;)
            local.get 2
            local.get 1
            i32.eq
            br_if 2 (;@2;)
            local.get 1
            i32.const 1
            i32.add
            local.set 9
            loop  ;; label = @5
              block  ;; label = @6
                local.get 2
                local.get 1
                i32.ge_u
                br_if 0 (;@6;)
                local.get 0
                local.get 2
                i32.add
                i32.load8_s
                i32.const -64
                i32.ge_s
                br_if 4 (;@2;)
              end
              local.get 2
              i32.const -1
              i32.add
              local.set 8
              local.get 2
              i32.const 1
              i32.eq
              br_if 4 (;@1;)
              local.get 9
              local.get 2
              i32.eq
              local.set 6
              local.get 8
              local.set 2
              local.get 6
              i32.eqz
              br_if 0 (;@5;)
              br 4 (;@1;)
            end
          end
          local.get 4
          local.get 2
          local.get 3
          local.get 8
          select
          i32.store offset=40
          local.get 4
          i32.const 48
          i32.add
          i32.const 20
          i32.add
          i32.const 3
          i32.store
          local.get 4
          i32.const 72
          i32.add
          i32.const 20
          i32.add
          i32.const 79
          i32.store
          local.get 4
          i32.const 84
          i32.add
          i32.const 79
          i32.store
          local.get 4
          i64.const 3
          i64.store offset=52 align=4
          local.get 4
          i32.const 1055468
          i32.store offset=48
          local.get 4
          i32.const 18
          i32.store offset=76
          local.get 4
          local.get 4
          i32.const 72
          i32.add
          i32.store offset=64
          local.get 4
          local.get 4
          i32.const 24
          i32.add
          i32.store offset=88
          local.get 4
          local.get 4
          i32.const 16
          i32.add
          i32.store offset=80
          local.get 4
          local.get 4
          i32.const 40
          i32.add
          i32.store offset=72
          local.get 4
          i32.const 48
          i32.add
          i32.const 1055492
          call $_ZN4core9panicking9panic_fmt17h98142caac1112f39E
          unreachable
        end
        local.get 4
        i32.const 100
        i32.add
        i32.const 79
        i32.store
        local.get 4
        i32.const 72
        i32.add
        i32.const 20
        i32.add
        i32.const 79
        i32.store
        local.get 4
        i32.const 84
        i32.add
        i32.const 18
        i32.store
        local.get 4
        i32.const 48
        i32.add
        i32.const 20
        i32.add
        i32.const 4
        i32.store
        local.get 4
        i64.const 4
        i64.store offset=52 align=4
        local.get 4
        i32.const 1055544
        i32.store offset=48
        local.get 4
        i32.const 18
        i32.store offset=76
        local.get 4
        local.get 4
        i32.const 72
        i32.add
        i32.store offset=64
        local.get 4
        local.get 4
        i32.const 24
        i32.add
        i32.store offset=96
        local.get 4
        local.get 4
        i32.const 16
        i32.add
        i32.store offset=88
        local.get 4
        local.get 4
        i32.const 12
        i32.add
        i32.store offset=80
        local.get 4
        local.get 4
        i32.const 8
        i32.add
        i32.store offset=72
        local.get 4
        i32.const 48
        i32.add
        i32.const 1055576
        call $_ZN4core9panicking9panic_fmt17h98142caac1112f39E
        unreachable
      end
      local.get 2
      local.set 8
    end
    block  ;; label = @1
      local.get 8
      local.get 1
      i32.eq
      br_if 0 (;@1;)
      i32.const 1
      local.set 6
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              local.get 0
              local.get 8
              i32.add
              local.tee 9
              i32.load8_s
              local.tee 2
              i32.const -1
              i32.gt_s
              br_if 0 (;@5;)
              i32.const 0
              local.set 5
              local.get 0
              local.get 1
              i32.add
              local.tee 6
              local.set 1
              block  ;; label = @6
                local.get 9
                i32.const 1
                i32.add
                local.get 6
                i32.eq
                br_if 0 (;@6;)
                local.get 9
                i32.const 2
                i32.add
                local.set 1
                local.get 9
                i32.load8_u offset=1
                i32.const 63
                i32.and
                local.set 5
              end
              local.get 2
              i32.const 31
              i32.and
              local.set 9
              local.get 2
              i32.const 255
              i32.and
              i32.const 223
              i32.gt_u
              br_if 1 (;@4;)
              local.get 5
              local.get 9
              i32.const 6
              i32.shl
              i32.or
              local.set 1
              br 2 (;@3;)
            end
            local.get 4
            local.get 2
            i32.const 255
            i32.and
            i32.store offset=36
            local.get 4
            i32.const 40
            i32.add
            local.set 2
            br 2 (;@2;)
          end
          i32.const 0
          local.set 0
          local.get 6
          local.set 7
          block  ;; label = @4
            local.get 1
            local.get 6
            i32.eq
            br_if 0 (;@4;)
            local.get 1
            i32.const 1
            i32.add
            local.set 7
            local.get 1
            i32.load8_u
            i32.const 63
            i32.and
            local.set 0
          end
          local.get 0
          local.get 5
          i32.const 6
          i32.shl
          i32.or
          local.set 1
          block  ;; label = @4
            local.get 2
            i32.const 255
            i32.and
            i32.const 240
            i32.ge_u
            br_if 0 (;@4;)
            local.get 1
            local.get 9
            i32.const 12
            i32.shl
            i32.or
            local.set 1
            br 1 (;@3;)
          end
          i32.const 0
          local.set 2
          block  ;; label = @4
            local.get 7
            local.get 6
            i32.eq
            br_if 0 (;@4;)
            local.get 7
            i32.load8_u
            i32.const 63
            i32.and
            local.set 2
          end
          local.get 1
          i32.const 6
          i32.shl
          local.get 9
          i32.const 18
          i32.shl
          i32.const 1835008
          i32.and
          i32.or
          local.get 2
          i32.or
          local.tee 1
          i32.const 1114112
          i32.eq
          br_if 2 (;@1;)
        end
        local.get 4
        local.get 1
        i32.store offset=36
        i32.const 1
        local.set 6
        local.get 4
        i32.const 40
        i32.add
        local.set 2
        local.get 1
        i32.const 128
        i32.lt_u
        br_if 0 (;@2;)
        i32.const 2
        local.set 6
        local.get 1
        i32.const 2048
        i32.lt_u
        br_if 0 (;@2;)
        i32.const 3
        i32.const 4
        local.get 1
        i32.const 65536
        i32.lt_u
        select
        local.set 6
      end
      local.get 4
      local.get 8
      i32.store offset=40
      local.get 4
      local.get 6
      local.get 8
      i32.add
      i32.store offset=44
      local.get 4
      i32.const 48
      i32.add
      i32.const 20
      i32.add
      i32.const 5
      i32.store
      local.get 4
      i32.const 108
      i32.add
      i32.const 79
      i32.store
      local.get 4
      i32.const 100
      i32.add
      i32.const 79
      i32.store
      local.get 4
      i32.const 72
      i32.add
      i32.const 20
      i32.add
      i32.const 80
      i32.store
      local.get 4
      i32.const 84
      i32.add
      i32.const 81
      i32.store
      local.get 4
      i64.const 5
      i64.store offset=52 align=4
      local.get 4
      i32.const 1055660
      i32.store offset=48
      local.get 4
      local.get 2
      i32.store offset=88
      local.get 4
      i32.const 18
      i32.store offset=76
      local.get 4
      local.get 4
      i32.const 72
      i32.add
      i32.store offset=64
      local.get 4
      local.get 4
      i32.const 24
      i32.add
      i32.store offset=104
      local.get 4
      local.get 4
      i32.const 16
      i32.add
      i32.store offset=96
      local.get 4
      local.get 4
      i32.const 36
      i32.add
      i32.store offset=80
      local.get 4
      local.get 4
      i32.const 32
      i32.add
      i32.store offset=72
      local.get 4
      i32.const 48
      i32.add
      i32.const 1055700
      call $_ZN4core9panicking9panic_fmt17h98142caac1112f39E
      unreachable
    end
    i32.const 1054389
    i32.const 43
    i32.const 1055592
    call $_ZN4core9panicking5panic17he9463ceb3e2615beE
    unreachable)
  (func $_ZN4core9panicking9panic_fmt17h98142caac1112f39E (type 4) (param i32 i32)
    (local i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 1
    i32.store offset=12
    local.get 2
    local.get 0
    i32.store offset=8
    local.get 2
    i32.const 1054452
    i32.store offset=4
    local.get 2
    i32.const 1054352
    i32.store
    local.get 2
    call $rust_begin_unwind
    unreachable)
  (func $_ZN4core3fmt3num3imp52_$LT$impl$u20$core..fmt..Display$u20$for$u20$u32$GT$3fmt17h976c74654a4bcc54E (type 2) (param i32 i32) (result i32)
    local.get 0
    i64.load32_u
    i32.const 1
    local.get 1
    call $_ZN4core3fmt3num3imp7fmt_u6417h035a0daf9e6f2b5cE)
  (func $_ZN4core3fmt5write17h0de1fe9fbd7990abE (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    i32.const 36
    i32.add
    local.get 1
    i32.store
    local.get 3
    i32.const 3
    i32.store8 offset=40
    local.get 3
    i64.const 137438953472
    i64.store offset=8
    local.get 3
    local.get 0
    i32.store offset=32
    i32.const 0
    local.set 4
    local.get 3
    i32.const 0
    i32.store offset=24
    local.get 3
    i32.const 0
    i32.store offset=16
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 2
            i32.load offset=8
            local.tee 5
            br_if 0 (;@4;)
            local.get 2
            i32.load
            local.set 6
            local.get 2
            i32.load offset=4
            local.tee 7
            local.get 2
            i32.const 20
            i32.add
            i32.load
            local.tee 8
            local.get 8
            local.get 7
            i32.gt_u
            select
            local.tee 9
            i32.eqz
            br_if 1 (;@3;)
            local.get 2
            i32.load offset=16
            local.set 2
            i32.const 1
            local.set 8
            local.get 0
            local.get 6
            i32.load
            local.get 6
            i32.load offset=4
            local.get 1
            i32.load offset=12
            call_indirect (type 6)
            br_if 3 (;@1;)
            local.get 6
            i32.const 12
            i32.add
            local.set 0
            i32.const 1
            local.set 4
            loop  ;; label = @5
              block  ;; label = @6
                local.get 2
                i32.load
                local.get 3
                i32.const 8
                i32.add
                local.get 2
                i32.const 4
                i32.add
                i32.load
                call_indirect (type 2)
                i32.eqz
                br_if 0 (;@6;)
                i32.const 1
                local.set 8
                br 5 (;@1;)
              end
              local.get 4
              local.get 9
              i32.ge_u
              br_if 2 (;@3;)
              local.get 0
              i32.const -4
              i32.add
              local.set 1
              local.get 0
              i32.load
              local.set 5
              local.get 0
              i32.const 8
              i32.add
              local.set 0
              local.get 2
              i32.const 8
              i32.add
              local.set 2
              i32.const 1
              local.set 8
              local.get 4
              i32.const 1
              i32.add
              local.set 4
              local.get 3
              i32.load offset=32
              local.get 1
              i32.load
              local.get 5
              local.get 3
              i32.load offset=36
              i32.load offset=12
              call_indirect (type 6)
              i32.eqz
              br_if 0 (;@5;)
              br 4 (;@1;)
            end
          end
          local.get 2
          i32.load
          local.set 6
          local.get 2
          i32.load offset=4
          local.tee 7
          local.get 2
          i32.const 12
          i32.add
          i32.load
          local.tee 8
          local.get 8
          local.get 7
          i32.gt_u
          select
          local.tee 10
          i32.eqz
          br_if 0 (;@3;)
          local.get 2
          i32.const 20
          i32.add
          i32.load
          local.set 9
          local.get 2
          i32.load offset=16
          local.set 11
          i32.const 1
          local.set 8
          local.get 0
          local.get 6
          i32.load
          local.get 6
          i32.load offset=4
          local.get 1
          i32.load offset=12
          call_indirect (type 6)
          br_if 2 (;@1;)
          local.get 6
          i32.const 12
          i32.add
          local.set 0
          local.get 5
          i32.const 16
          i32.add
          local.set 2
          i32.const 1
          local.set 4
          loop  ;; label = @4
            local.get 3
            local.get 2
            i32.const -12
            i32.add
            i32.load
            i32.store offset=12
            local.get 3
            local.get 2
            i32.const 12
            i32.add
            i32.load8_u
            i32.store8 offset=40
            local.get 3
            local.get 2
            i32.const -8
            i32.add
            i32.load
            i32.store offset=8
            i32.const 0
            local.set 5
            i32.const 0
            local.set 1
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    local.get 2
                    i32.const 4
                    i32.add
                    i32.load
                    br_table 1 (;@7;) 0 (;@8;) 3 (;@5;) 1 (;@7;)
                  end
                  block  ;; label = @8
                    local.get 2
                    i32.const 8
                    i32.add
                    i32.load
                    local.tee 12
                    local.get 9
                    i32.ge_u
                    br_if 0 (;@8;)
                    i32.const 0
                    local.set 1
                    local.get 11
                    local.get 12
                    i32.const 3
                    i32.shl
                    i32.add
                    local.tee 12
                    i32.load offset=4
                    i32.const 82
                    i32.ne
                    br_if 3 (;@5;)
                    local.get 12
                    i32.load
                    i32.load
                    local.set 8
                    br 2 (;@6;)
                  end
                  i32.const 1054896
                  local.get 12
                  local.get 9
                  call $_ZN4core9panicking18panic_bounds_check17h8c4f03235e0b5d9bE
                  unreachable
                end
                local.get 2
                i32.const 8
                i32.add
                i32.load
                local.set 8
              end
              i32.const 1
              local.set 1
            end
            local.get 3
            local.get 8
            i32.store offset=20
            local.get 3
            local.get 1
            i32.store offset=16
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    local.get 2
                    i32.const -4
                    i32.add
                    i32.load
                    br_table 1 (;@7;) 0 (;@8;) 3 (;@5;) 1 (;@7;)
                  end
                  block  ;; label = @8
                    local.get 2
                    i32.load
                    local.tee 1
                    local.get 9
                    i32.ge_u
                    br_if 0 (;@8;)
                    local.get 11
                    local.get 1
                    i32.const 3
                    i32.shl
                    i32.add
                    local.tee 1
                    i32.load offset=4
                    i32.const 82
                    i32.ne
                    br_if 3 (;@5;)
                    local.get 1
                    i32.load
                    i32.load
                    local.set 8
                    br 2 (;@6;)
                  end
                  i32.const 1054896
                  local.get 1
                  local.get 9
                  call $_ZN4core9panicking18panic_bounds_check17h8c4f03235e0b5d9bE
                  unreachable
                end
                local.get 2
                i32.load
                local.set 8
              end
              i32.const 1
              local.set 5
            end
            local.get 3
            local.get 8
            i32.store offset=28
            local.get 3
            local.get 5
            i32.store offset=24
            local.get 2
            i32.const -16
            i32.add
            i32.load
            local.tee 8
            local.get 9
            i32.ge_u
            br_if 2 (;@2;)
            block  ;; label = @5
              local.get 11
              local.get 8
              i32.const 3
              i32.shl
              i32.add
              local.tee 8
              i32.load
              local.get 3
              i32.const 8
              i32.add
              local.get 8
              i32.load offset=4
              call_indirect (type 2)
              i32.eqz
              br_if 0 (;@5;)
              i32.const 1
              local.set 8
              br 4 (;@1;)
            end
            local.get 4
            local.get 10
            i32.ge_u
            br_if 1 (;@3;)
            local.get 0
            i32.const -4
            i32.add
            local.set 1
            local.get 0
            i32.load
            local.set 5
            local.get 0
            i32.const 8
            i32.add
            local.set 0
            local.get 2
            i32.const 32
            i32.add
            local.set 2
            i32.const 1
            local.set 8
            local.get 4
            i32.const 1
            i32.add
            local.set 4
            local.get 3
            i32.load offset=32
            local.get 1
            i32.load
            local.get 5
            local.get 3
            i32.load offset=36
            i32.load offset=12
            call_indirect (type 6)
            i32.eqz
            br_if 0 (;@4;)
            br 3 (;@1;)
          end
        end
        block  ;; label = @3
          local.get 7
          local.get 4
          i32.le_u
          br_if 0 (;@3;)
          i32.const 1
          local.set 8
          local.get 3
          i32.load offset=32
          local.get 6
          local.get 4
          i32.const 3
          i32.shl
          i32.add
          local.tee 2
          i32.load
          local.get 2
          i32.load offset=4
          local.get 3
          i32.load offset=36
          i32.load offset=12
          call_indirect (type 6)
          br_if 2 (;@1;)
        end
        i32.const 0
        local.set 8
        br 1 (;@1;)
      end
      i32.const 1054880
      local.get 8
      local.get 9
      call $_ZN4core9panicking18panic_bounds_check17h8c4f03235e0b5d9bE
      unreachable
    end
    local.get 3
    i32.const 48
    i32.add
    global.set 0
    local.get 8)
  (func $_ZN71_$LT$core..ops..range..Range$LT$Idx$GT$$u20$as$u20$core..fmt..Debug$GT$3fmt17h9736df68fe193a38E (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 2
    global.set 0
    block  ;; label = @1
      local.get 0
      local.get 1
      call $_ZN4core3fmt3num52_$LT$impl$u20$core..fmt..Debug$u20$for$u20$usize$GT$3fmt17h65f5d7159ee75f4fE
      br_if 0 (;@1;)
      local.get 1
      i32.const 28
      i32.add
      i32.load
      local.set 3
      local.get 1
      i32.load offset=24
      local.set 4
      local.get 2
      i32.const 28
      i32.add
      i32.const 0
      i32.store
      local.get 2
      i32.const 1054352
      i32.store offset=24
      local.get 2
      i64.const 1
      i64.store offset=12 align=4
      local.get 2
      i32.const 1054356
      i32.store offset=8
      local.get 4
      local.get 3
      local.get 2
      i32.const 8
      i32.add
      call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
      br_if 0 (;@1;)
      local.get 0
      i32.const 4
      i32.add
      local.get 1
      call $_ZN4core3fmt3num52_$LT$impl$u20$core..fmt..Debug$u20$for$u20$usize$GT$3fmt17h65f5d7159ee75f4fE
      local.set 1
      local.get 2
      i32.const 32
      i32.add
      global.set 0
      local.get 1
      return
    end
    local.get 2
    i32.const 32
    i32.add
    global.set 0
    i32.const 1)
  (func $_ZN4core3fmt3num52_$LT$impl$u20$core..fmt..Debug$u20$for$u20$usize$GT$3fmt17h65f5d7159ee75f4fE (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 128
    i32.sub
    local.tee 2
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              local.get 1
              i32.load
              local.tee 3
              i32.const 16
              i32.and
              br_if 0 (;@5;)
              local.get 0
              i32.load
              local.set 4
              local.get 3
              i32.const 32
              i32.and
              br_if 1 (;@4;)
              local.get 4
              i64.extend_i32_u
              i32.const 1
              local.get 1
              call $_ZN4core3fmt3num3imp7fmt_u6417h035a0daf9e6f2b5cE
              local.set 0
              br 2 (;@3;)
            end
            local.get 0
            i32.load
            local.set 4
            i32.const 0
            local.set 0
            loop  ;; label = @5
              local.get 2
              local.get 0
              i32.add
              i32.const 127
              i32.add
              local.get 4
              i32.const 15
              i32.and
              local.tee 3
              i32.const 48
              i32.or
              local.get 3
              i32.const 87
              i32.add
              local.get 3
              i32.const 10
              i32.lt_u
              select
              i32.store8
              local.get 0
              i32.const -1
              i32.add
              local.set 0
              local.get 4
              i32.const 4
              i32.shr_u
              local.tee 4
              br_if 0 (;@5;)
            end
            local.get 0
            i32.const 128
            i32.add
            local.tee 4
            i32.const 129
            i32.ge_u
            br_if 2 (;@2;)
            local.get 1
            i32.const 1
            i32.const 1054629
            i32.const 2
            local.get 2
            local.get 0
            i32.add
            i32.const 128
            i32.add
            i32.const 0
            local.get 0
            i32.sub
            call $_ZN4core3fmt9Formatter12pad_integral17he52ae3771fdc9ec7E
            local.set 0
            br 1 (;@3;)
          end
          i32.const 0
          local.set 0
          loop  ;; label = @4
            local.get 2
            local.get 0
            i32.add
            i32.const 127
            i32.add
            local.get 4
            i32.const 15
            i32.and
            local.tee 3
            i32.const 48
            i32.or
            local.get 3
            i32.const 55
            i32.add
            local.get 3
            i32.const 10
            i32.lt_u
            select
            i32.store8
            local.get 0
            i32.const -1
            i32.add
            local.set 0
            local.get 4
            i32.const 4
            i32.shr_u
            local.tee 4
            br_if 0 (;@4;)
          end
          local.get 0
          i32.const 128
          i32.add
          local.tee 4
          i32.const 129
          i32.ge_u
          br_if 2 (;@1;)
          local.get 1
          i32.const 1
          i32.const 1054629
          i32.const 2
          local.get 2
          local.get 0
          i32.add
          i32.const 128
          i32.add
          i32.const 0
          local.get 0
          i32.sub
          call $_ZN4core3fmt9Formatter12pad_integral17he52ae3771fdc9ec7E
          local.set 0
        end
        local.get 2
        i32.const 128
        i32.add
        global.set 0
        local.get 0
        return
      end
      local.get 4
      i32.const 128
      call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
      unreachable
    end
    local.get 4
    i32.const 128
    call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
    unreachable)
  (func $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17hb5955542a46d6244E (type 1) (param i32) (result i64)
    i64.const 794850088668468598)
  (func $_ZN60_$LT$core..cell..BorrowError$u20$as$u20$core..fmt..Debug$GT$3fmt17hddc186737bf29b56E (type 2) (param i32 i32) (result i32)
    local.get 1
    i32.load offset=24
    i32.const 1054364
    i32.const 11
    local.get 1
    i32.const 28
    i32.add
    i32.load
    i32.load offset=12
    call_indirect (type 6))
  (func $_ZN63_$LT$core..cell..BorrowMutError$u20$as$u20$core..fmt..Debug$GT$3fmt17h77bcc250a44c6ae5E (type 2) (param i32 i32) (result i32)
    local.get 1
    i32.load offset=24
    i32.const 1054375
    i32.const 14
    local.get 1
    i32.const 28
    i32.add
    i32.load
    i32.load offset=12
    call_indirect (type 6))
  (func $_ZN4core4char7methods22_$LT$impl$u20$char$GT$16escape_debug_ext17hb85592115fdc6803E (type 5) (param i32 i32 i32)
    (local i32 i32 i32 i64)
    i32.const 2
    local.set 3
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              local.get 1
              i32.const -9
              i32.add
              local.tee 4
              i32.const 30
              i32.le_u
              br_if 0 (;@5;)
              local.get 1
              i32.const 92
              i32.ne
              br_if 1 (;@4;)
              br 2 (;@3;)
            end
            i32.const 116
            local.set 5
            block  ;; label = @5
              block  ;; label = @6
                local.get 4
                br_table 5 (;@1;) 1 (;@5;) 2 (;@4;) 2 (;@4;) 0 (;@6;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 3 (;@3;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 2 (;@4;) 3 (;@3;) 5 (;@1;)
              end
              i32.const 114
              local.set 5
              br 4 (;@1;)
            end
            i32.const 110
            local.set 5
            br 3 (;@1;)
          end
          block  ;; label = @4
            local.get 2
            i32.eqz
            br_if 0 (;@4;)
            local.get 1
            i32.const 10
            i32.shr_u
            local.set 5
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    local.get 1
                    i32.const 125952
                    i32.lt_u
                    br_if 0 (;@8;)
                    i32.const 30
                    local.set 3
                    local.get 5
                    i32.const 896
                    i32.ne
                    br_if 4 (;@4;)
                    br 1 (;@7;)
                  end
                  local.get 5
                  i32.const 1057232
                  i32.add
                  i32.load8_u
                  local.tee 3
                  i32.const 30
                  i32.gt_u
                  br_if 1 (;@6;)
                end
                local.get 3
                i32.const 4
                i32.shl
                local.get 1
                i32.const 6
                i32.shr_u
                i32.const 15
                i32.and
                i32.or
                i32.const 1057355
                i32.add
                i32.load8_u
                local.tee 5
                i32.const 139
                i32.ge_u
                br_if 1 (;@5;)
                i32.const 3
                local.set 3
                local.get 5
                i32.const 3
                i32.shl
                i32.const 1057856
                i32.add
                i64.load
                i64.const 1
                local.get 1
                i32.const 63
                i32.and
                i64.extend_i32_u
                i64.shl
                i64.and
                i64.eqz
                br_if 2 (;@4;)
                local.get 1
                i32.const 1
                i32.or
                i32.clz
                i32.const 2
                i32.shr_u
                i32.const 7
                i32.xor
                i64.extend_i32_u
                i64.const 21474836480
                i64.or
                local.set 6
                br 4 (;@2;)
              end
              i32.const 1057124
              local.get 3
              i32.const 31
              call $_ZN4core9panicking18panic_bounds_check17h8c4f03235e0b5d9bE
              unreachable
            end
            i32.const 1057140
            local.get 5
            i32.const 139
            call $_ZN4core9panicking18panic_bounds_check17h8c4f03235e0b5d9bE
            unreachable
          end
          block  ;; label = @4
            local.get 1
            call $_ZN4core7unicode9printable12is_printable17h28b8beadd74ff247E
            i32.eqz
            br_if 0 (;@4;)
            i32.const 1
            local.set 3
            br 2 (;@2;)
          end
          local.get 1
          i32.const 1
          i32.or
          i32.clz
          i32.const 2
          i32.shr_u
          i32.const 7
          i32.xor
          i64.extend_i32_u
          i64.const 21474836480
          i64.or
          local.set 6
          i32.const 3
          local.set 3
          br 1 (;@2;)
        end
      end
      local.get 1
      local.set 5
    end
    local.get 0
    local.get 5
    i32.store offset=4
    local.get 0
    local.get 3
    i32.store
    local.get 0
    i32.const 8
    i32.add
    local.get 6
    i64.store align=4)
  (func $_ZN4core7unicode9printable12is_printable17h28b8beadd74ff247E (type 14) (param i32) (result i32)
    (local i32)
    block  ;; label = @1
      local.get 0
      i32.const 65536
      i32.lt_u
      br_if 0 (;@1;)
      block  ;; label = @2
        block  ;; label = @3
          local.get 0
          i32.const 131072
          i32.lt_u
          br_if 0 (;@3;)
          i32.const 0
          local.set 1
          local.get 0
          i32.const -195102
          i32.add
          i32.const 722658
          i32.lt_u
          br_if 1 (;@2;)
          local.get 0
          i32.const -191457
          i32.add
          i32.const 3103
          i32.lt_u
          br_if 1 (;@2;)
          local.get 0
          i32.const -183970
          i32.add
          i32.const 14
          i32.lt_u
          br_if 1 (;@2;)
          local.get 0
          i32.const 2097150
          i32.and
          i32.const 178206
          i32.eq
          br_if 1 (;@2;)
          local.get 0
          i32.const -173783
          i32.add
          i32.const 41
          i32.lt_u
          br_if 1 (;@2;)
          local.get 0
          i32.const -177973
          i32.add
          i32.const 11
          i32.lt_u
          br_if 1 (;@2;)
          local.get 0
          i32.const -918000
          i32.add
          i32.const 196111
          i32.gt_u
          return
        end
        local.get 0
        i32.const 1056453
        i32.const 35
        i32.const 1056523
        i32.const 166
        i32.const 1056689
        i32.const 408
        call $_ZN4core7unicode9printable5check17h32a85d89e686515aE
        local.set 1
      end
      local.get 1
      return
    end
    local.get 0
    i32.const 1055764
    i32.const 41
    i32.const 1055846
    i32.const 293
    i32.const 1056139
    i32.const 314
    call $_ZN4core7unicode9printable5check17h32a85d89e686515aE)
  (func $_ZN82_$LT$core..char..EscapeDebug$u20$as$u20$core..iter..traits..iterator..Iterator$GT$4next17h8d8e9bdd03c8beb6E (type 14) (param i32) (result i32)
    (local i32 i32)
    i32.const 1114112
    local.set 1
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 0
            i32.load
            br_table 3 (;@1;) 1 (;@3;) 0 (;@4;) 2 (;@2;) 3 (;@1;)
          end
          local.get 0
          i32.const 1
          i32.store
          i32.const 92
          return
        end
        local.get 0
        i32.const 0
        i32.store
        local.get 0
        i32.load offset=4
        return
      end
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                local.get 0
                i32.const 12
                i32.add
                i32.load8_u
                br_table 5 (;@1;) 4 (;@2;) 3 (;@3;) 2 (;@4;) 1 (;@5;) 0 (;@6;) 5 (;@1;)
              end
              local.get 0
              i32.const 4
              i32.store8 offset=12
              i32.const 92
              return
            end
            local.get 0
            i32.const 3
            i32.store8 offset=12
            i32.const 117
            return
          end
          local.get 0
          i32.const 2
          i32.store8 offset=12
          i32.const 123
          return
        end
        local.get 0
        i32.load offset=4
        local.get 0
        i32.const 8
        i32.add
        i32.load
        local.tee 2
        i32.const 2
        i32.shl
        i32.const 28
        i32.and
        i32.shr_u
        i32.const 15
        i32.and
        local.tee 1
        i32.const 48
        i32.or
        local.get 1
        i32.const 87
        i32.add
        local.get 1
        i32.const 10
        i32.lt_u
        select
        local.set 1
        block  ;; label = @3
          local.get 2
          i32.eqz
          br_if 0 (;@3;)
          local.get 0
          local.get 2
          i32.const -1
          i32.add
          i32.store offset=8
          local.get 1
          return
        end
        local.get 0
        i32.const 1
        i32.store8 offset=12
        local.get 1
        return
      end
      local.get 0
      i32.const 0
      i32.store8 offset=12
      i32.const 125
      local.set 1
    end
    local.get 1)
  (func $_ZN4core3fmt8builders11DebugStruct5field17hdb0382cc3deab674E (type 15) (param i32 i32 i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i64 i64)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 5
    global.set 0
    i32.const 1
    local.set 6
    block  ;; label = @1
      local.get 0
      i32.load8_u offset=4
      br_if 0 (;@1;)
      local.get 0
      i32.load8_u offset=5
      local.set 7
      block  ;; label = @2
        local.get 0
        i32.load
        local.tee 8
        i32.load8_u
        i32.const 4
        i32.and
        br_if 0 (;@2;)
        i32.const 1
        local.set 6
        local.get 8
        i32.load offset=24
        i32.const 1054597
        i32.const 1054599
        local.get 7
        i32.const 255
        i32.and
        local.tee 7
        select
        i32.const 2
        i32.const 3
        local.get 7
        select
        local.get 8
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        br_if 1 (;@1;)
        i32.const 1
        local.set 6
        local.get 0
        i32.load
        local.tee 8
        i32.load offset=24
        local.get 1
        local.get 2
        local.get 8
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        br_if 1 (;@1;)
        i32.const 1
        local.set 6
        local.get 0
        i32.load
        local.tee 8
        i32.load offset=24
        i32.const 1054432
        i32.const 2
        local.get 8
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        br_if 1 (;@1;)
        local.get 3
        local.get 0
        i32.load
        local.get 4
        i32.load offset=12
        call_indirect (type 2)
        local.set 6
        br 1 (;@1;)
      end
      block  ;; label = @2
        local.get 7
        i32.const 255
        i32.and
        br_if 0 (;@2;)
        i32.const 1
        local.set 6
        local.get 8
        i32.load offset=24
        i32.const 1054592
        i32.const 3
        local.get 8
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        br_if 1 (;@1;)
        local.get 0
        i32.load
        local.set 8
      end
      i32.const 1
      local.set 6
      local.get 5
      i32.const 1
      i32.store8 offset=23
      local.get 5
      i32.const 52
      i32.add
      i32.const 1054564
      i32.store
      local.get 5
      local.get 8
      i64.load offset=24 align=4
      i64.store offset=8
      local.get 5
      local.get 5
      i32.const 23
      i32.add
      i32.store offset=16
      local.get 8
      i64.load offset=8 align=4
      local.set 9
      local.get 8
      i64.load offset=16 align=4
      local.set 10
      local.get 5
      local.get 8
      i32.load8_u offset=32
      i32.store8 offset=56
      local.get 5
      local.get 10
      i64.store offset=40
      local.get 5
      local.get 9
      i64.store offset=32
      local.get 5
      local.get 8
      i64.load align=4
      i64.store offset=24
      local.get 5
      local.get 5
      i32.const 8
      i32.add
      i32.store offset=48
      local.get 5
      i32.const 8
      i32.add
      local.get 1
      local.get 2
      call $_ZN68_$LT$core..fmt..builders..PadAdapter$u20$as$u20$core..fmt..Write$GT$9write_str17hc704ee84ceffabf7E
      br_if 0 (;@1;)
      local.get 5
      i32.const 8
      i32.add
      i32.const 1054432
      i32.const 2
      call $_ZN68_$LT$core..fmt..builders..PadAdapter$u20$as$u20$core..fmt..Write$GT$9write_str17hc704ee84ceffabf7E
      br_if 0 (;@1;)
      local.get 3
      local.get 5
      i32.const 24
      i32.add
      local.get 4
      i32.load offset=12
      call_indirect (type 2)
      br_if 0 (;@1;)
      local.get 5
      i32.load offset=48
      i32.const 1054595
      i32.const 2
      local.get 5
      i32.load offset=52
      i32.load offset=12
      call_indirect (type 6)
      local.set 6
    end
    local.get 0
    i32.const 1
    i32.store8 offset=5
    local.get 0
    local.get 6
    i32.store8 offset=4
    local.get 5
    i32.const 64
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN44_$LT$$RF$T$u20$as$u20$core..fmt..Display$GT$3fmt17h51bafaa45edf02f5E (type 2) (param i32 i32) (result i32)
    local.get 1
    local.get 0
    i32.load
    local.get 0
    i32.load offset=4
    call $_ZN4core3fmt9Formatter3pad17h7b301e85900e29c6E)
  (func $_ZN4core6option18expect_none_failed17h5718e8afd751d0acE (type 12) (param i32 i32 i32 i32 i32)
    (local i32)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 5
    global.set 0
    local.get 5
    local.get 1
    i32.store offset=12
    local.get 5
    local.get 0
    i32.store offset=8
    local.get 5
    local.get 3
    i32.store offset=20
    local.get 5
    local.get 2
    i32.store offset=16
    local.get 5
    i32.const 44
    i32.add
    i32.const 2
    i32.store
    local.get 5
    i32.const 60
    i32.add
    i32.const 83
    i32.store
    local.get 5
    i64.const 2
    i64.store offset=28 align=4
    local.get 5
    i32.const 1054436
    i32.store offset=24
    local.get 5
    i32.const 79
    i32.store offset=52
    local.get 5
    local.get 5
    i32.const 48
    i32.add
    i32.store offset=40
    local.get 5
    local.get 5
    i32.const 16
    i32.add
    i32.store offset=56
    local.get 5
    local.get 5
    i32.const 8
    i32.add
    i32.store offset=48
    local.get 5
    i32.const 24
    i32.add
    local.get 4
    call $_ZN4core9panicking9panic_fmt17h98142caac1112f39E
    unreachable)
  (func $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h343baa976fb42c89E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 1
    local.get 0
    i32.load offset=4
    i32.load offset=12
    call_indirect (type 2))
  (func $_ZN4core5panic9PanicInfo7message17hffa6f3d3e6ff0a39E (type 14) (param i32) (result i32)
    local.get 0
    i32.load offset=8)
  (func $_ZN4core5panic9PanicInfo8location17h30a49797d3e7ef56E (type 14) (param i32) (result i32)
    local.get 0
    i32.load offset=12)
  (func $_ZN4core5panic8Location6caller17hba7ec45f0d210bdeE (type 14) (param i32) (result i32)
    local.get 0)
  (func $_ZN4core5panic8Location4file17h54f5698a3003e7b3E (type 4) (param i32 i32)
    local.get 0
    local.get 1
    i64.load align=4
    i64.store align=4)
  (func $_ZN60_$LT$core..panic..Location$u20$as$u20$core..fmt..Display$GT$3fmt17h79c3ef5172e6604aE (type 2) (param i32 i32) (result i32)
    (local i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    i32.const 20
    i32.add
    i32.const 18
    i32.store
    local.get 2
    i32.const 12
    i32.add
    i32.const 18
    i32.store
    local.get 2
    i32.const 79
    i32.store offset=4
    local.get 2
    local.get 0
    i32.store
    local.get 2
    local.get 0
    i32.const 12
    i32.add
    i32.store offset=16
    local.get 2
    local.get 0
    i32.const 8
    i32.add
    i32.store offset=8
    local.get 1
    i32.const 28
    i32.add
    i32.load
    local.set 0
    local.get 1
    i32.load offset=24
    local.set 1
    local.get 2
    i32.const 24
    i32.add
    i32.const 20
    i32.add
    i32.const 3
    i32.store
    local.get 2
    i64.const 3
    i64.store offset=28 align=4
    local.get 2
    i32.const 1054472
    i32.store offset=24
    local.get 2
    local.get 2
    i32.store offset=40
    local.get 1
    local.get 0
    local.get 2
    i32.const 24
    i32.add
    call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
    local.set 0
    local.get 2
    i32.const 48
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN68_$LT$core..fmt..builders..PadAdapter$u20$as$u20$core..fmt..Write$GT$9write_str17hc704ee84ceffabf7E (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 3
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        br_if 0 (;@2;)
        i32.const 0
        local.set 4
        br 1 (;@1;)
      end
      local.get 3
      i32.const 40
      i32.add
      local.set 5
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              loop  ;; label = @6
                block  ;; label = @7
                  local.get 0
                  i32.load offset=8
                  i32.load8_u
                  i32.eqz
                  br_if 0 (;@7;)
                  local.get 0
                  i32.load
                  i32.const 1054588
                  i32.const 4
                  local.get 0
                  i32.load offset=4
                  i32.load offset=12
                  call_indirect (type 6)
                  br_if 5 (;@2;)
                end
                local.get 3
                i32.const 10
                i32.store offset=40
                local.get 3
                i64.const 4294967306
                i64.store offset=32
                local.get 3
                local.get 2
                i32.store offset=28
                local.get 3
                i32.const 0
                i32.store offset=24
                local.get 3
                local.get 2
                i32.store offset=20
                local.get 3
                local.get 1
                i32.store offset=16
                local.get 3
                i32.const 8
                i32.add
                i32.const 10
                local.get 1
                local.get 2
                call $_ZN4core5slice6memchr6memchr17h5dbdc97a74440bacE
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      block  ;; label = @10
                        local.get 3
                        i32.load offset=8
                        i32.const 1
                        i32.ne
                        br_if 0 (;@10;)
                        local.get 3
                        i32.load offset=12
                        local.set 4
                        loop  ;; label = @11
                          local.get 3
                          local.get 4
                          local.get 3
                          i32.load offset=24
                          i32.add
                          i32.const 1
                          i32.add
                          local.tee 4
                          i32.store offset=24
                          block  ;; label = @12
                            block  ;; label = @13
                              local.get 4
                              local.get 3
                              i32.load offset=36
                              local.tee 6
                              i32.ge_u
                              br_if 0 (;@13;)
                              local.get 3
                              i32.load offset=20
                              local.set 7
                              br 1 (;@12;)
                            end
                            local.get 3
                            i32.load offset=20
                            local.tee 7
                            local.get 4
                            i32.lt_u
                            br_if 0 (;@12;)
                            local.get 6
                            i32.const 5
                            i32.ge_u
                            br_if 7 (;@5;)
                            local.get 3
                            i32.load offset=16
                            local.get 4
                            local.get 6
                            i32.sub
                            local.tee 8
                            i32.add
                            local.tee 9
                            local.get 5
                            i32.eq
                            br_if 4 (;@8;)
                            local.get 9
                            local.get 5
                            local.get 6
                            call $memcmp
                            i32.eqz
                            br_if 4 (;@8;)
                          end
                          local.get 3
                          i32.load offset=28
                          local.tee 9
                          local.get 4
                          i32.lt_u
                          br_if 2 (;@9;)
                          local.get 7
                          local.get 9
                          i32.lt_u
                          br_if 2 (;@9;)
                          local.get 3
                          local.get 6
                          local.get 3
                          i32.const 16
                          i32.add
                          i32.add
                          i32.const 23
                          i32.add
                          i32.load8_u
                          local.get 3
                          i32.load offset=16
                          local.get 4
                          i32.add
                          local.get 9
                          local.get 4
                          i32.sub
                          call $_ZN4core5slice6memchr6memchr17h5dbdc97a74440bacE
                          local.get 3
                          i32.load offset=4
                          local.set 4
                          local.get 3
                          i32.load
                          i32.const 1
                          i32.eq
                          br_if 0 (;@11;)
                        end
                      end
                      local.get 3
                      local.get 3
                      i32.load offset=28
                      i32.store offset=24
                    end
                    local.get 0
                    i32.load offset=8
                    i32.const 0
                    i32.store8
                    local.get 2
                    local.set 4
                    br 1 (;@7;)
                  end
                  local.get 0
                  i32.load offset=8
                  i32.const 1
                  i32.store8
                  local.get 8
                  i32.const 1
                  i32.add
                  local.set 4
                end
                local.get 0
                i32.load offset=4
                local.set 9
                local.get 0
                i32.load
                local.set 6
                block  ;; label = @7
                  local.get 4
                  i32.eqz
                  local.get 2
                  local.get 4
                  i32.eq
                  i32.or
                  local.tee 7
                  br_if 0 (;@7;)
                  local.get 2
                  local.get 4
                  i32.le_u
                  br_if 3 (;@4;)
                  local.get 1
                  local.get 4
                  i32.add
                  i32.load8_s
                  i32.const -65
                  i32.le_s
                  br_if 3 (;@4;)
                end
                local.get 6
                local.get 1
                local.get 4
                local.get 9
                i32.load offset=12
                call_indirect (type 6)
                br_if 4 (;@2;)
                block  ;; label = @7
                  local.get 7
                  br_if 0 (;@7;)
                  local.get 2
                  local.get 4
                  i32.le_u
                  br_if 4 (;@3;)
                  local.get 1
                  local.get 4
                  i32.add
                  i32.load8_s
                  i32.const -65
                  i32.le_s
                  br_if 4 (;@3;)
                end
                local.get 1
                local.get 4
                i32.add
                local.set 1
                local.get 2
                local.get 4
                i32.sub
                local.tee 2
                br_if 0 (;@6;)
              end
              i32.const 0
              local.set 4
              br 4 (;@1;)
            end
            local.get 6
            i32.const 4
            call $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E
            unreachable
          end
          local.get 1
          local.get 2
          i32.const 0
          local.get 4
          call $_ZN4core3str16slice_error_fail17ha06f3354b25aeac4E
          unreachable
        end
        local.get 1
        local.get 2
        local.get 4
        local.get 2
        call $_ZN4core3str16slice_error_fail17ha06f3354b25aeac4E
        unreachable
      end
      i32.const 1
      local.set 4
    end
    local.get 3
    i32.const 48
    i32.add
    global.set 0
    local.get 4)
  (func $_ZN4core5slice6memchr6memchr17h5dbdc97a74440bacE (type 3) (param i32 i32 i32 i32)
    (local i32 i32 i32 i32 i32 i32)
    i32.const 0
    local.set 4
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.const 3
        i32.and
        local.tee 5
        i32.eqz
        br_if 0 (;@2;)
        i32.const 4
        local.get 5
        i32.sub
        local.tee 5
        i32.eqz
        br_if 0 (;@2;)
        local.get 3
        local.get 5
        local.get 5
        local.get 3
        i32.gt_u
        select
        local.set 4
        i32.const 0
        local.set 5
        local.get 1
        i32.const 255
        i32.and
        local.set 6
        loop  ;; label = @3
          local.get 4
          local.get 5
          i32.eq
          br_if 1 (;@2;)
          local.get 2
          local.get 5
          i32.add
          local.set 7
          local.get 5
          i32.const 1
          i32.add
          local.set 5
          local.get 7
          i32.load8_u
          local.tee 7
          local.get 6
          i32.ne
          br_if 0 (;@3;)
        end
        i32.const 1
        local.set 3
        local.get 7
        local.get 1
        i32.const 255
        i32.and
        i32.eq
        i32.const 1
        i32.add
        i32.const 1
        i32.and
        local.get 5
        i32.add
        i32.const -1
        i32.add
        local.set 5
        br 1 (;@1;)
      end
      local.get 1
      i32.const 255
      i32.and
      local.set 6
      block  ;; label = @2
        block  ;; label = @3
          local.get 3
          i32.const 8
          i32.lt_u
          br_if 0 (;@3;)
          local.get 4
          local.get 3
          i32.const -8
          i32.add
          local.tee 8
          i32.gt_u
          br_if 0 (;@3;)
          local.get 6
          i32.const 16843009
          i32.mul
          local.set 5
          block  ;; label = @4
            loop  ;; label = @5
              local.get 2
              local.get 4
              i32.add
              local.tee 7
              i32.const 4
              i32.add
              i32.load
              local.get 5
              i32.xor
              local.tee 9
              i32.const -1
              i32.xor
              local.get 9
              i32.const -16843009
              i32.add
              i32.and
              local.get 7
              i32.load
              local.get 5
              i32.xor
              local.tee 7
              i32.const -1
              i32.xor
              local.get 7
              i32.const -16843009
              i32.add
              i32.and
              i32.or
              i32.const -2139062144
              i32.and
              br_if 1 (;@4;)
              local.get 4
              i32.const 8
              i32.add
              local.tee 4
              local.get 8
              i32.le_u
              br_if 0 (;@5;)
            end
          end
          local.get 4
          local.get 3
          i32.gt_u
          br_if 1 (;@2;)
        end
        local.get 2
        local.get 4
        i32.add
        local.set 9
        local.get 3
        local.get 4
        i32.sub
        local.set 2
        i32.const 0
        local.set 3
        i32.const 0
        local.set 5
        block  ;; label = @3
          loop  ;; label = @4
            local.get 2
            local.get 5
            i32.eq
            br_if 1 (;@3;)
            local.get 9
            local.get 5
            i32.add
            local.set 7
            local.get 5
            i32.const 1
            i32.add
            local.set 5
            local.get 7
            i32.load8_u
            local.tee 7
            local.get 6
            i32.ne
            br_if 0 (;@4;)
          end
          i32.const 1
          local.set 3
          local.get 7
          local.get 1
          i32.const 255
          i32.and
          i32.eq
          i32.const 1
          i32.add
          i32.const 1
          i32.and
          local.get 5
          i32.add
          i32.const -1
          i32.add
          local.set 5
        end
        local.get 5
        local.get 4
        i32.add
        local.set 5
        br 1 (;@1;)
      end
      local.get 4
      local.get 3
      call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
      unreachable
    end
    local.get 0
    local.get 5
    i32.store offset=4
    local.get 0
    local.get 3
    i32.store)
  (func $_ZN4core3fmt8builders10DebugTuple5field17h95b19566bf4f9168E (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i64 i64)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 3
    global.set 0
    i32.const 1
    local.set 4
    block  ;; label = @1
      local.get 0
      i32.load8_u offset=8
      br_if 0 (;@1;)
      local.get 0
      i32.load offset=4
      local.set 5
      block  ;; label = @2
        local.get 0
        i32.load
        local.tee 6
        i32.load8_u
        i32.const 4
        i32.and
        br_if 0 (;@2;)
        i32.const 1
        local.set 4
        local.get 6
        i32.load offset=24
        i32.const 1054597
        i32.const 1054607
        local.get 5
        select
        i32.const 2
        i32.const 1
        local.get 5
        select
        local.get 6
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        br_if 1 (;@1;)
        local.get 1
        local.get 0
        i32.load
        local.get 2
        i32.load offset=12
        call_indirect (type 2)
        local.set 4
        br 1 (;@1;)
      end
      block  ;; label = @2
        local.get 5
        br_if 0 (;@2;)
        i32.const 1
        local.set 4
        local.get 6
        i32.load offset=24
        i32.const 1054605
        i32.const 2
        local.get 6
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        br_if 1 (;@1;)
        local.get 0
        i32.load
        local.set 6
      end
      i32.const 1
      local.set 4
      local.get 3
      i32.const 1
      i32.store8 offset=23
      local.get 3
      i32.const 52
      i32.add
      i32.const 1054564
      i32.store
      local.get 3
      local.get 6
      i64.load offset=24 align=4
      i64.store offset=8
      local.get 3
      local.get 3
      i32.const 23
      i32.add
      i32.store offset=16
      local.get 6
      i64.load offset=8 align=4
      local.set 7
      local.get 6
      i64.load offset=16 align=4
      local.set 8
      local.get 3
      local.get 6
      i32.load8_u offset=32
      i32.store8 offset=56
      local.get 3
      local.get 8
      i64.store offset=40
      local.get 3
      local.get 7
      i64.store offset=32
      local.get 3
      local.get 6
      i64.load align=4
      i64.store offset=24
      local.get 3
      local.get 3
      i32.const 8
      i32.add
      i32.store offset=48
      local.get 1
      local.get 3
      i32.const 24
      i32.add
      local.get 2
      i32.load offset=12
      call_indirect (type 2)
      br_if 0 (;@1;)
      local.get 3
      i32.load offset=48
      i32.const 1054595
      i32.const 2
      local.get 3
      i32.load offset=52
      i32.load offset=12
      call_indirect (type 6)
      local.set 4
    end
    local.get 0
    local.get 4
    i32.store8 offset=8
    local.get 0
    local.get 0
    i32.load offset=4
    i32.const 1
    i32.add
    i32.store offset=4
    local.get 3
    i32.const 64
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN4core3fmt8builders10DebugTuple6finish17hd8ce6586f49c209fE (type 14) (param i32) (result i32)
    (local i32 i32 i32)
    local.get 0
    i32.load8_u offset=8
    local.set 1
    block  ;; label = @1
      local.get 0
      i32.load offset=4
      local.tee 2
      i32.eqz
      br_if 0 (;@1;)
      local.get 1
      i32.const 255
      i32.and
      local.set 3
      i32.const 1
      local.set 1
      block  ;; label = @2
        local.get 3
        br_if 0 (;@2;)
        block  ;; label = @3
          local.get 2
          i32.const 1
          i32.ne
          br_if 0 (;@3;)
          local.get 0
          i32.load8_u offset=9
          i32.eqz
          br_if 0 (;@3;)
          local.get 0
          i32.load
          local.tee 3
          i32.load8_u
          i32.const 4
          i32.and
          br_if 0 (;@3;)
          i32.const 1
          local.set 1
          local.get 3
          i32.load offset=24
          i32.const 1054608
          i32.const 1
          local.get 3
          i32.const 28
          i32.add
          i32.load
          i32.load offset=12
          call_indirect (type 6)
          br_if 1 (;@2;)
        end
        local.get 0
        i32.load
        local.tee 1
        i32.load offset=24
        i32.const 1054609
        i32.const 1
        local.get 1
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        local.set 1
      end
      local.get 0
      local.get 1
      i32.store8 offset=8
    end
    local.get 1
    i32.const 255
    i32.and
    i32.const 0
    i32.ne)
  (func $_ZN4core3fmt8builders10DebugInner5entry17h009755ce841ea300E (type 5) (param i32 i32 i32)
    (local i32 i32 i32 i64 i64)
    global.get 0
    i32.const 64
    i32.sub
    local.tee 3
    global.set 0
    i32.const 1
    local.set 4
    block  ;; label = @1
      local.get 0
      i32.load8_u offset=4
      br_if 0 (;@1;)
      local.get 0
      i32.load8_u offset=5
      local.set 4
      block  ;; label = @2
        local.get 0
        i32.load
        local.tee 5
        i32.load8_u
        i32.const 4
        i32.and
        br_if 0 (;@2;)
        block  ;; label = @3
          local.get 4
          i32.const 255
          i32.and
          i32.eqz
          br_if 0 (;@3;)
          i32.const 1
          local.set 4
          local.get 5
          i32.load offset=24
          i32.const 1054597
          i32.const 2
          local.get 5
          i32.const 28
          i32.add
          i32.load
          i32.load offset=12
          call_indirect (type 6)
          br_if 2 (;@1;)
          local.get 0
          i32.load
          local.set 5
        end
        local.get 1
        local.get 5
        local.get 2
        i32.load offset=12
        call_indirect (type 2)
        local.set 4
        br 1 (;@1;)
      end
      block  ;; label = @2
        local.get 4
        i32.const 255
        i32.and
        br_if 0 (;@2;)
        i32.const 1
        local.set 4
        local.get 5
        i32.load offset=24
        i32.const 1054610
        i32.const 1
        local.get 5
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        br_if 1 (;@1;)
        local.get 0
        i32.load
        local.set 5
      end
      i32.const 1
      local.set 4
      local.get 3
      i32.const 1
      i32.store8 offset=23
      local.get 3
      i32.const 52
      i32.add
      i32.const 1054564
      i32.store
      local.get 3
      local.get 5
      i64.load offset=24 align=4
      i64.store offset=8
      local.get 3
      local.get 3
      i32.const 23
      i32.add
      i32.store offset=16
      local.get 5
      i64.load offset=8 align=4
      local.set 6
      local.get 5
      i64.load offset=16 align=4
      local.set 7
      local.get 3
      local.get 5
      i32.load8_u offset=32
      i32.store8 offset=56
      local.get 3
      local.get 7
      i64.store offset=40
      local.get 3
      local.get 6
      i64.store offset=32
      local.get 3
      local.get 5
      i64.load align=4
      i64.store offset=24
      local.get 3
      local.get 3
      i32.const 8
      i32.add
      i32.store offset=48
      local.get 1
      local.get 3
      i32.const 24
      i32.add
      local.get 2
      i32.load offset=12
      call_indirect (type 2)
      br_if 0 (;@1;)
      local.get 3
      i32.load offset=48
      i32.const 1054595
      i32.const 2
      local.get 3
      i32.load offset=52
      i32.load offset=12
      call_indirect (type 6)
      local.set 4
    end
    local.get 0
    i32.const 1
    i32.store8 offset=5
    local.get 0
    local.get 4
    i32.store8 offset=4
    local.get 3
    i32.const 64
    i32.add
    global.set 0)
  (func $_ZN4core3fmt8builders8DebugSet5entry17ha23dddb04336d96cE (type 6) (param i32 i32 i32) (result i32)
    local.get 0
    local.get 1
    local.get 2
    call $_ZN4core3fmt8builders10DebugInner5entry17h009755ce841ea300E
    local.get 0)
  (func $_ZN4core3fmt8builders9DebugList6finish17h6732b39c5e331c0aE (type 14) (param i32) (result i32)
    (local i32)
    i32.const 1
    local.set 1
    block  ;; label = @1
      local.get 0
      i32.load8_u offset=4
      br_if 0 (;@1;)
      local.get 0
      i32.load
      local.tee 0
      i32.load offset=24
      i32.const 1054628
      i32.const 1
      local.get 0
      i32.const 28
      i32.add
      i32.load
      i32.load offset=12
      call_indirect (type 6)
      local.set 1
    end
    local.get 1)
  (func $_ZN4core3fmt5Write10write_char17hbc1fcddc054f7d7fE (type 2) (param i32 i32) (result i32)
    (local i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    i32.const 0
    i32.store offset=12
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 1
            i32.const 128
            i32.lt_u
            br_if 0 (;@4;)
            local.get 1
            i32.const 2048
            i32.lt_u
            br_if 1 (;@3;)
            local.get 2
            i32.const 12
            i32.add
            local.set 3
            local.get 1
            i32.const 65536
            i32.ge_u
            br_if 2 (;@2;)
            local.get 2
            local.get 1
            i32.const 63
            i32.and
            i32.const 128
            i32.or
            i32.store8 offset=14
            local.get 2
            local.get 1
            i32.const 6
            i32.shr_u
            i32.const 63
            i32.and
            i32.const 128
            i32.or
            i32.store8 offset=13
            local.get 2
            local.get 1
            i32.const 12
            i32.shr_u
            i32.const 15
            i32.and
            i32.const 224
            i32.or
            i32.store8 offset=12
            i32.const 3
            local.set 1
            br 3 (;@1;)
          end
          local.get 2
          local.get 1
          i32.store8 offset=12
          local.get 2
          i32.const 12
          i32.add
          local.set 3
          i32.const 1
          local.set 1
          br 2 (;@1;)
        end
        local.get 2
        local.get 1
        i32.const 63
        i32.and
        i32.const 128
        i32.or
        i32.store8 offset=13
        local.get 2
        local.get 1
        i32.const 6
        i32.shr_u
        i32.const 31
        i32.and
        i32.const 192
        i32.or
        i32.store8 offset=12
        local.get 2
        i32.const 12
        i32.add
        local.set 3
        i32.const 2
        local.set 1
        br 1 (;@1;)
      end
      local.get 2
      local.get 1
      i32.const 63
      i32.and
      i32.const 128
      i32.or
      i32.store8 offset=15
      local.get 2
      local.get 1
      i32.const 18
      i32.shr_u
      i32.const 240
      i32.or
      i32.store8 offset=12
      local.get 2
      local.get 1
      i32.const 6
      i32.shr_u
      i32.const 63
      i32.and
      i32.const 128
      i32.or
      i32.store8 offset=14
      local.get 2
      local.get 1
      i32.const 12
      i32.shr_u
      i32.const 63
      i32.and
      i32.const 128
      i32.or
      i32.store8 offset=13
      i32.const 4
      local.set 1
    end
    local.get 0
    local.get 3
    local.get 1
    call $_ZN68_$LT$core..fmt..builders..PadAdapter$u20$as$u20$core..fmt..Write$GT$9write_str17hc704ee84ceffabf7E
    local.set 1
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN4core3fmt5Write9write_fmt17h0a230916a217709bE (type 2) (param i32 i32) (result i32)
    (local i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 0
    i32.store offset=4
    local.get 2
    i32.const 8
    i32.add
    i32.const 16
    i32.add
    local.get 1
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    i32.const 8
    i32.add
    i32.const 8
    i32.add
    local.get 1
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    local.get 1
    i64.load align=4
    i64.store offset=8
    local.get 2
    i32.const 4
    i32.add
    i32.const 1054832
    local.get 2
    i32.const 8
    i32.add
    call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
    local.set 1
    local.get 2
    i32.const 32
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_str17h0d9b9fc37108a31fE (type 6) (param i32 i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 1
    local.get 2
    call $_ZN68_$LT$core..fmt..builders..PadAdapter$u20$as$u20$core..fmt..Write$GT$9write_str17hc704ee84ceffabf7E)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$10write_char17h5e8659a6ef4b958fE (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 1
    call $_ZN4core3fmt5Write10write_char17hbc1fcddc054f7d7fE)
  (func $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_fmt17h3342b5868002fad5E (type 2) (param i32 i32) (result i32)
    (local i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 2
    global.set 0
    local.get 2
    local.get 0
    i32.load
    i32.store offset=4
    local.get 2
    i32.const 8
    i32.add
    i32.const 16
    i32.add
    local.get 1
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    i32.const 8
    i32.add
    i32.const 8
    i32.add
    local.get 1
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    local.get 1
    i64.load align=4
    i64.store offset=8
    local.get 2
    i32.const 4
    i32.add
    i32.const 1054832
    local.get 2
    i32.const 8
    i32.add
    call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
    local.set 1
    local.get 2
    i32.const 32
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN4core3fmt10ArgumentV110show_usize17hc56a657681711f3eE (type 2) (param i32 i32) (result i32)
    local.get 0
    i64.load32_u
    i32.const 1
    local.get 1
    call $_ZN4core3fmt3num3imp7fmt_u6417h035a0daf9e6f2b5cE)
  (func $_ZN4core3fmt3num3imp7fmt_u6417h035a0daf9e6f2b5cE (type 16) (param i64 i32 i32) (result i32)
    (local i32 i32 i64 i32 i32 i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 3
    global.set 0
    i32.const 39
    local.set 4
    block  ;; label = @1
      block  ;; label = @2
        local.get 0
        i64.const 10000
        i64.ge_u
        br_if 0 (;@2;)
        local.get 0
        local.set 5
        br 1 (;@1;)
      end
      i32.const 39
      local.set 4
      loop  ;; label = @2
        local.get 3
        i32.const 9
        i32.add
        local.get 4
        i32.add
        local.tee 6
        i32.const -4
        i32.add
        local.get 0
        local.get 0
        i64.const 10000
        i64.div_u
        local.tee 5
        i64.const 10000
        i64.mul
        i64.sub
        i32.wrap_i64
        local.tee 7
        i32.const 65535
        i32.and
        i32.const 100
        i32.div_u
        local.tee 8
        i32.const 1
        i32.shl
        i32.const 1054631
        i32.add
        i32.load16_u align=1
        i32.store16 align=1
        local.get 6
        i32.const -2
        i32.add
        local.get 7
        local.get 8
        i32.const 100
        i32.mul
        i32.sub
        i32.const 65535
        i32.and
        i32.const 1
        i32.shl
        i32.const 1054631
        i32.add
        i32.load16_u align=1
        i32.store16 align=1
        local.get 4
        i32.const -4
        i32.add
        local.set 4
        local.get 0
        i64.const 99999999
        i64.gt_u
        local.set 6
        local.get 5
        local.set 0
        local.get 6
        br_if 0 (;@2;)
      end
    end
    block  ;; label = @1
      local.get 5
      i32.wrap_i64
      local.tee 6
      i32.const 99
      i32.le_s
      br_if 0 (;@1;)
      local.get 3
      i32.const 9
      i32.add
      local.get 4
      i32.const -2
      i32.add
      local.tee 4
      i32.add
      local.get 5
      i32.wrap_i64
      local.tee 6
      local.get 6
      i32.const 65535
      i32.and
      i32.const 100
      i32.div_u
      local.tee 6
      i32.const 100
      i32.mul
      i32.sub
      i32.const 65535
      i32.and
      i32.const 1
      i32.shl
      i32.const 1054631
      i32.add
      i32.load16_u align=1
      i32.store16 align=1
    end
    block  ;; label = @1
      block  ;; label = @2
        local.get 6
        i32.const 10
        i32.lt_s
        br_if 0 (;@2;)
        local.get 3
        i32.const 9
        i32.add
        local.get 4
        i32.const -2
        i32.add
        local.tee 4
        i32.add
        local.get 6
        i32.const 1
        i32.shl
        i32.const 1054631
        i32.add
        i32.load16_u align=1
        i32.store16 align=1
        br 1 (;@1;)
      end
      local.get 3
      i32.const 9
      i32.add
      local.get 4
      i32.const -1
      i32.add
      local.tee 4
      i32.add
      local.get 6
      i32.const 48
      i32.add
      i32.store8
    end
    local.get 2
    local.get 1
    i32.const 1054352
    i32.const 0
    local.get 3
    i32.const 9
    i32.add
    local.get 4
    i32.add
    i32.const 39
    local.get 4
    i32.sub
    call $_ZN4core3fmt9Formatter12pad_integral17he52ae3771fdc9ec7E
    local.set 4
    local.get 3
    i32.const 48
    i32.add
    global.set 0
    local.get 4)
  (func $_ZN59_$LT$core..fmt..Arguments$u20$as$u20$core..fmt..Display$GT$3fmt17hd7c46cc3e94bd933E (type 2) (param i32 i32) (result i32)
    (local i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 2
    global.set 0
    local.get 1
    i32.const 28
    i32.add
    i32.load
    local.set 3
    local.get 1
    i32.load offset=24
    local.set 1
    local.get 2
    i32.const 8
    i32.add
    i32.const 16
    i32.add
    local.get 0
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    i32.const 8
    i32.add
    i32.const 8
    i32.add
    local.get 0
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    local.get 0
    i64.load align=4
    i64.store offset=8
    local.get 1
    local.get 3
    local.get 2
    i32.const 8
    i32.add
    call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
    local.set 0
    local.get 2
    i32.const 32
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN4core3fmt9Formatter12pad_integral17he52ae3771fdc9ec7E (type 17) (param i32 i32 i32 i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32)
    block  ;; label = @1
      block  ;; label = @2
        local.get 1
        i32.eqz
        br_if 0 (;@2;)
        i32.const 43
        i32.const 1114112
        local.get 0
        i32.load
        local.tee 6
        i32.const 1
        i32.and
        local.tee 1
        select
        local.set 7
        local.get 1
        local.get 5
        i32.add
        local.set 8
        br 1 (;@1;)
      end
      local.get 5
      i32.const 1
      i32.add
      local.set 8
      local.get 0
      i32.load
      local.set 6
      i32.const 45
      local.set 7
    end
    block  ;; label = @1
      block  ;; label = @2
        local.get 6
        i32.const 4
        i32.and
        br_if 0 (;@2;)
        i32.const 0
        local.set 2
        br 1 (;@1;)
      end
      i32.const 0
      local.set 9
      block  ;; label = @2
        local.get 3
        i32.eqz
        br_if 0 (;@2;)
        local.get 3
        local.set 10
        local.get 2
        local.set 1
        loop  ;; label = @3
          local.get 9
          local.get 1
          i32.load8_u
          i32.const 192
          i32.and
          i32.const 128
          i32.eq
          i32.add
          local.set 9
          local.get 1
          i32.const 1
          i32.add
          local.set 1
          local.get 10
          i32.const -1
          i32.add
          local.tee 10
          br_if 0 (;@3;)
        end
      end
      local.get 8
      local.get 3
      i32.add
      local.get 9
      i32.sub
      local.set 8
    end
    i32.const 1
    local.set 1
    block  ;; label = @1
      block  ;; label = @2
        local.get 0
        i32.load offset=8
        i32.const 1
        i32.eq
        br_if 0 (;@2;)
        local.get 0
        local.get 7
        local.get 2
        local.get 3
        call $_ZN4core3fmt9Formatter12pad_integral12write_prefix17h4fb635e7b8293773E
        br_if 1 (;@1;)
        local.get 0
        i32.load offset=24
        local.get 4
        local.get 5
        local.get 0
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        return
      end
      block  ;; label = @2
        local.get 0
        i32.const 12
        i32.add
        i32.load
        local.tee 9
        local.get 8
        i32.gt_u
        br_if 0 (;@2;)
        local.get 0
        local.get 7
        local.get 2
        local.get 3
        call $_ZN4core3fmt9Formatter12pad_integral12write_prefix17h4fb635e7b8293773E
        br_if 1 (;@1;)
        local.get 0
        i32.load offset=24
        local.get 4
        local.get 5
        local.get 0
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        return
      end
      block  ;; label = @2
        block  ;; label = @3
          local.get 6
          i32.const 8
          i32.and
          br_if 0 (;@3;)
          i32.const 0
          local.set 1
          local.get 9
          local.get 8
          i32.sub
          local.tee 9
          local.set 8
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                i32.const 1
                local.get 0
                i32.load8_u offset=32
                local.tee 10
                local.get 10
                i32.const 3
                i32.eq
                select
                br_table 2 (;@4;) 1 (;@5;) 0 (;@6;) 1 (;@5;) 2 (;@4;)
              end
              local.get 9
              i32.const 1
              i32.shr_u
              local.set 1
              local.get 9
              i32.const 1
              i32.add
              i32.const 1
              i32.shr_u
              local.set 8
              br 1 (;@4;)
            end
            i32.const 0
            local.set 8
            local.get 9
            local.set 1
          end
          local.get 1
          i32.const 1
          i32.add
          local.set 1
          loop  ;; label = @4
            local.get 1
            i32.const -1
            i32.add
            local.tee 1
            i32.eqz
            br_if 2 (;@2;)
            local.get 0
            i32.load offset=24
            local.get 0
            i32.load offset=4
            local.get 0
            i32.load offset=28
            i32.load offset=16
            call_indirect (type 2)
            i32.eqz
            br_if 0 (;@4;)
          end
          i32.const 1
          return
        end
        local.get 0
        i32.load offset=4
        local.set 6
        local.get 0
        i32.const 48
        i32.store offset=4
        local.get 0
        i32.load8_u offset=32
        local.set 11
        i32.const 1
        local.set 1
        local.get 0
        i32.const 1
        i32.store8 offset=32
        local.get 0
        local.get 7
        local.get 2
        local.get 3
        call $_ZN4core3fmt9Formatter12pad_integral12write_prefix17h4fb635e7b8293773E
        br_if 1 (;@1;)
        i32.const 0
        local.set 1
        local.get 9
        local.get 8
        i32.sub
        local.tee 10
        local.set 3
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              i32.const 1
              local.get 0
              i32.load8_u offset=32
              local.tee 9
              local.get 9
              i32.const 3
              i32.eq
              select
              br_table 2 (;@3;) 1 (;@4;) 0 (;@5;) 1 (;@4;) 2 (;@3;)
            end
            local.get 10
            i32.const 1
            i32.shr_u
            local.set 1
            local.get 10
            i32.const 1
            i32.add
            i32.const 1
            i32.shr_u
            local.set 3
            br 1 (;@3;)
          end
          i32.const 0
          local.set 3
          local.get 10
          local.set 1
        end
        local.get 1
        i32.const 1
        i32.add
        local.set 1
        block  ;; label = @3
          loop  ;; label = @4
            local.get 1
            i32.const -1
            i32.add
            local.tee 1
            i32.eqz
            br_if 1 (;@3;)
            local.get 0
            i32.load offset=24
            local.get 0
            i32.load offset=4
            local.get 0
            i32.load offset=28
            i32.load offset=16
            call_indirect (type 2)
            i32.eqz
            br_if 0 (;@4;)
          end
          i32.const 1
          return
        end
        local.get 0
        i32.load offset=4
        local.set 10
        i32.const 1
        local.set 1
        local.get 0
        i32.load offset=24
        local.get 4
        local.get 5
        local.get 0
        i32.load offset=28
        i32.load offset=12
        call_indirect (type 6)
        br_if 1 (;@1;)
        local.get 3
        i32.const 1
        i32.add
        local.set 9
        local.get 0
        i32.load offset=28
        local.set 3
        local.get 0
        i32.load offset=24
        local.set 2
        block  ;; label = @3
          loop  ;; label = @4
            local.get 9
            i32.const -1
            i32.add
            local.tee 9
            i32.eqz
            br_if 1 (;@3;)
            i32.const 1
            local.set 1
            local.get 2
            local.get 10
            local.get 3
            i32.load offset=16
            call_indirect (type 2)
            i32.eqz
            br_if 0 (;@4;)
            br 3 (;@1;)
          end
        end
        local.get 0
        local.get 11
        i32.store8 offset=32
        local.get 0
        local.get 6
        i32.store offset=4
        i32.const 0
        return
      end
      local.get 0
      i32.load offset=4
      local.set 10
      i32.const 1
      local.set 1
      local.get 0
      local.get 7
      local.get 2
      local.get 3
      call $_ZN4core3fmt9Formatter12pad_integral12write_prefix17h4fb635e7b8293773E
      br_if 0 (;@1;)
      local.get 0
      i32.load offset=24
      local.get 4
      local.get 5
      local.get 0
      i32.load offset=28
      i32.load offset=12
      call_indirect (type 6)
      br_if 0 (;@1;)
      local.get 8
      i32.const 1
      i32.add
      local.set 9
      local.get 0
      i32.load offset=28
      local.set 3
      local.get 0
      i32.load offset=24
      local.set 0
      loop  ;; label = @2
        block  ;; label = @3
          local.get 9
          i32.const -1
          i32.add
          local.tee 9
          br_if 0 (;@3;)
          i32.const 0
          return
        end
        i32.const 1
        local.set 1
        local.get 0
        local.get 10
        local.get 3
        i32.load offset=16
        call_indirect (type 2)
        i32.eqz
        br_if 0 (;@2;)
      end
    end
    local.get 1)
  (func $_ZN4core3fmt9Formatter12pad_integral12write_prefix17h4fb635e7b8293773E (type 8) (param i32 i32 i32 i32) (result i32)
    (local i32)
    block  ;; label = @1
      block  ;; label = @2
        local.get 1
        i32.const 1114112
        i32.eq
        br_if 0 (;@2;)
        i32.const 1
        local.set 4
        local.get 0
        i32.load offset=24
        local.get 1
        local.get 0
        i32.const 28
        i32.add
        i32.load
        i32.load offset=16
        call_indirect (type 2)
        br_if 1 (;@1;)
      end
      block  ;; label = @2
        local.get 2
        br_if 0 (;@2;)
        i32.const 0
        return
      end
      local.get 0
      i32.load offset=24
      local.get 2
      local.get 3
      local.get 0
      i32.const 28
      i32.add
      i32.load
      i32.load offset=12
      call_indirect (type 6)
      local.set 4
    end
    local.get 4)
  (func $_ZN4core3fmt9Formatter9write_str17h6367e5f885508b07E (type 6) (param i32 i32 i32) (result i32)
    local.get 0
    i32.load offset=24
    local.get 1
    local.get 2
    local.get 0
    i32.const 28
    i32.add
    i32.load
    i32.load offset=12
    call_indirect (type 6))
  (func $_ZN4core3fmt9Formatter9write_fmt17ha552aa6bb1a0a03bE (type 2) (param i32 i32) (result i32)
    (local i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 2
    global.set 0
    local.get 0
    i32.const 28
    i32.add
    i32.load
    local.set 3
    local.get 0
    i32.load offset=24
    local.set 0
    local.get 2
    i32.const 8
    i32.add
    i32.const 16
    i32.add
    local.get 1
    i32.const 16
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    i32.const 8
    i32.add
    i32.const 8
    i32.add
    local.get 1
    i32.const 8
    i32.add
    i64.load align=4
    i64.store
    local.get 2
    local.get 1
    i64.load align=4
    i64.store offset=8
    local.get 0
    local.get 3
    local.get 2
    i32.const 8
    i32.add
    call $_ZN4core3fmt5write17h0de1fe9fbd7990abE
    local.set 1
    local.get 2
    i32.const 32
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN4core3fmt9Formatter15debug_lower_hex17h56f7e617e2f0ca72E (type 14) (param i32) (result i32)
    local.get 0
    i32.load8_u
    i32.const 16
    i32.and
    i32.const 4
    i32.shr_u)
  (func $_ZN4core3fmt9Formatter15debug_upper_hex17h009e81a991e324e6E (type 14) (param i32) (result i32)
    local.get 0
    i32.load8_u
    i32.const 32
    i32.and
    i32.const 5
    i32.shr_u)
  (func $_ZN4core3fmt9Formatter11debug_tuple17h1c7dc8aa00b962f9E (type 3) (param i32 i32 i32 i32)
    local.get 0
    local.get 1
    i32.load offset=24
    local.get 2
    local.get 3
    local.get 1
    i32.const 28
    i32.add
    i32.load
    i32.load offset=12
    call_indirect (type 6)
    i32.store8 offset=8
    local.get 0
    local.get 1
    i32.store
    local.get 0
    local.get 3
    i32.eqz
    i32.store8 offset=9
    local.get 0
    i32.const 0
    i32.store offset=4)
  (func $_ZN4core3fmt9Formatter10debug_list17h1d7285676248dd4eE (type 4) (param i32 i32)
    (local i32)
    local.get 1
    i32.load offset=24
    i32.const 1054611
    i32.const 1
    local.get 1
    i32.const 28
    i32.add
    i32.load
    i32.load offset=12
    call_indirect (type 6)
    local.set 2
    local.get 0
    i32.const 0
    i32.store8 offset=5
    local.get 0
    local.get 2
    i32.store8 offset=4
    local.get 0
    local.get 1
    i32.store)
  (func $_ZN57_$LT$core..fmt..Formatter$u20$as$u20$core..fmt..Write$GT$10write_char17ha4e8098fcbda3807E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load offset=24
    local.get 1
    local.get 0
    i32.const 28
    i32.add
    i32.load
    i32.load offset=16
    call_indirect (type 2))
  (func $_ZN40_$LT$str$u20$as$u20$core..fmt..Debug$GT$3fmt17h1a54fd8ecae06fe9E (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee 3
    global.set 0
    i32.const 1
    local.set 4
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.load offset=24
        i32.const 34
        local.get 2
        i32.const 28
        i32.add
        i32.load
        i32.load offset=16
        call_indirect (type 2)
        br_if 0 (;@2;)
        block  ;; label = @3
          block  ;; label = @4
            local.get 1
            br_if 0 (;@4;)
            i32.const 0
            local.set 5
            br 1 (;@3;)
          end
          local.get 0
          local.get 1
          i32.add
          local.set 6
          local.get 0
          local.set 7
          i32.const 0
          local.set 5
          i32.const 0
          local.set 8
          block  ;; label = @4
            loop  ;; label = @5
              local.get 7
              local.set 9
              local.get 7
              i32.const 1
              i32.add
              local.set 10
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    local.get 7
                    i32.load8_s
                    local.tee 11
                    i32.const -1
                    i32.gt_s
                    br_if 0 (;@8;)
                    block  ;; label = @9
                      block  ;; label = @10
                        local.get 10
                        local.get 6
                        i32.ne
                        br_if 0 (;@10;)
                        i32.const 0
                        local.set 12
                        local.get 6
                        local.set 7
                        br 1 (;@9;)
                      end
                      local.get 7
                      i32.load8_u offset=1
                      i32.const 63
                      i32.and
                      local.set 12
                      local.get 7
                      i32.const 2
                      i32.add
                      local.tee 10
                      local.set 7
                    end
                    local.get 11
                    i32.const 31
                    i32.and
                    local.set 13
                    block  ;; label = @9
                      local.get 11
                      i32.const 255
                      i32.and
                      local.tee 11
                      i32.const 223
                      i32.gt_u
                      br_if 0 (;@9;)
                      local.get 12
                      local.get 13
                      i32.const 6
                      i32.shl
                      i32.or
                      local.set 12
                      br 2 (;@7;)
                    end
                    block  ;; label = @9
                      block  ;; label = @10
                        local.get 7
                        local.get 6
                        i32.ne
                        br_if 0 (;@10;)
                        i32.const 0
                        local.set 14
                        local.get 6
                        local.set 15
                        br 1 (;@9;)
                      end
                      local.get 7
                      i32.load8_u
                      i32.const 63
                      i32.and
                      local.set 14
                      local.get 7
                      i32.const 1
                      i32.add
                      local.tee 10
                      local.set 15
                    end
                    local.get 14
                    local.get 12
                    i32.const 6
                    i32.shl
                    i32.or
                    local.set 12
                    block  ;; label = @9
                      local.get 11
                      i32.const 240
                      i32.ge_u
                      br_if 0 (;@9;)
                      local.get 12
                      local.get 13
                      i32.const 12
                      i32.shl
                      i32.or
                      local.set 12
                      br 2 (;@7;)
                    end
                    block  ;; label = @9
                      block  ;; label = @10
                        local.get 15
                        local.get 6
                        i32.ne
                        br_if 0 (;@10;)
                        i32.const 0
                        local.set 11
                        local.get 10
                        local.set 7
                        br 1 (;@9;)
                      end
                      local.get 15
                      i32.const 1
                      i32.add
                      local.set 7
                      local.get 15
                      i32.load8_u
                      i32.const 63
                      i32.and
                      local.set 11
                    end
                    local.get 12
                    i32.const 6
                    i32.shl
                    local.get 13
                    i32.const 18
                    i32.shl
                    i32.const 1835008
                    i32.and
                    i32.or
                    local.get 11
                    i32.or
                    local.tee 12
                    i32.const 1114112
                    i32.ne
                    br_if 2 (;@6;)
                    br 4 (;@4;)
                  end
                  local.get 11
                  i32.const 255
                  i32.and
                  local.set 12
                end
                local.get 10
                local.set 7
              end
              local.get 3
              local.get 12
              i32.const 1
              call $_ZN4core4char7methods22_$LT$impl$u20$char$GT$16escape_debug_ext17hb85592115fdc6803E
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      local.get 3
                      i32.load
                      local.tee 10
                      br_table 1 (;@8;) 2 (;@7;) 1 (;@8;) 0 (;@9;) 1 (;@8;)
                    end
                    local.get 3
                    i32.load offset=8
                    local.get 3
                    i32.load8_u offset=12
                    i32.add
                    i32.const 1
                    i32.eq
                    br_if 1 (;@7;)
                  end
                  local.get 3
                  local.get 1
                  i32.store offset=20
                  local.get 3
                  local.get 0
                  i32.store offset=16
                  local.get 3
                  local.get 5
                  i32.store offset=24
                  local.get 3
                  local.get 8
                  i32.store offset=28
                  block  ;; label = @8
                    block  ;; label = @9
                      local.get 8
                      local.get 5
                      i32.lt_u
                      br_if 0 (;@9;)
                      block  ;; label = @10
                        local.get 5
                        i32.eqz
                        br_if 0 (;@10;)
                        local.get 5
                        local.get 1
                        i32.eq
                        br_if 0 (;@10;)
                        local.get 5
                        local.get 1
                        i32.ge_u
                        br_if 1 (;@9;)
                        local.get 0
                        local.get 5
                        i32.add
                        i32.load8_s
                        i32.const -65
                        i32.le_s
                        br_if 1 (;@9;)
                      end
                      block  ;; label = @10
                        local.get 8
                        i32.eqz
                        br_if 0 (;@10;)
                        local.get 8
                        local.get 1
                        i32.eq
                        br_if 0 (;@10;)
                        local.get 8
                        local.get 1
                        i32.ge_u
                        br_if 1 (;@9;)
                        local.get 0
                        local.get 8
                        i32.add
                        i32.load8_s
                        i32.const -65
                        i32.le_s
                        br_if 1 (;@9;)
                      end
                      local.get 2
                      i32.load offset=24
                      local.get 0
                      local.get 5
                      i32.add
                      local.get 8
                      local.get 5
                      i32.sub
                      local.get 2
                      i32.load offset=28
                      i32.load offset=12
                      call_indirect (type 6)
                      i32.eqz
                      br_if 1 (;@8;)
                      br 3 (;@6;)
                    end
                    local.get 3
                    local.get 3
                    i32.const 28
                    i32.add
                    i32.store offset=40
                    local.get 3
                    local.get 3
                    i32.const 24
                    i32.add
                    i32.store offset=36
                    local.get 3
                    local.get 3
                    i32.const 16
                    i32.add
                    i32.store offset=32
                    local.get 3
                    i32.const 32
                    i32.add
                    call $_ZN4core3str6traits101_$LT$impl$u20$core..slice..SliceIndex$LT$str$GT$$u20$for$u20$core..ops..range..Range$LT$usize$GT$$GT$5index28_$u7b$$u7b$closure$u7d$$u7d$17h019af69a3de9894cE
                    unreachable
                  end
                  local.get 3
                  i32.load8_u offset=12
                  local.set 13
                  local.get 3
                  i32.load offset=8
                  local.set 14
                  local.get 3
                  i32.load offset=4
                  local.set 15
                  loop  ;; label = @8
                    local.get 10
                    local.set 11
                    i32.const 1
                    local.set 10
                    i32.const 92
                    local.set 5
                    block  ;; label = @9
                      block  ;; label = @10
                        block  ;; label = @11
                          block  ;; label = @12
                            block  ;; label = @13
                              local.get 11
                              br_table 2 (;@11;) 1 (;@12;) 4 (;@9;) 0 (;@13;) 2 (;@11;)
                            end
                            local.get 13
                            i32.const 255
                            i32.and
                            local.set 11
                            i32.const 3
                            local.set 10
                            i32.const 4
                            local.set 13
                            block  ;; label = @13
                              block  ;; label = @14
                                block  ;; label = @15
                                  local.get 11
                                  br_table 4 (;@11;) 2 (;@13;) 1 (;@14;) 0 (;@15;) 5 (;@10;) 6 (;@9;) 4 (;@11;)
                                end
                                i32.const 2
                                local.set 13
                                i32.const 123
                                local.set 5
                                br 5 (;@9;)
                              end
                              local.get 15
                              local.get 14
                              i32.const 2
                              i32.shl
                              i32.const 28
                              i32.and
                              i32.shr_u
                              i32.const 15
                              i32.and
                              local.tee 5
                              i32.const 48
                              i32.or
                              local.get 5
                              i32.const 87
                              i32.add
                              local.get 5
                              i32.const 10
                              i32.lt_u
                              select
                              local.set 5
                              i32.const 2
                              i32.const 1
                              local.get 14
                              select
                              local.set 13
                              local.get 14
                              i32.const -1
                              i32.add
                              i32.const 0
                              local.get 14
                              select
                              local.set 14
                              br 4 (;@9;)
                            end
                            i32.const 0
                            local.set 13
                            i32.const 125
                            local.set 5
                            br 3 (;@9;)
                          end
                          i32.const 0
                          local.set 10
                          local.get 15
                          local.set 5
                          local.get 15
                          i32.const 1114112
                          i32.ne
                          br_if 2 (;@9;)
                        end
                        i32.const 1
                        local.set 10
                        block  ;; label = @11
                          local.get 12
                          i32.const 128
                          i32.lt_u
                          br_if 0 (;@11;)
                          i32.const 2
                          local.set 10
                          local.get 12
                          i32.const 2048
                          i32.lt_u
                          br_if 0 (;@11;)
                          i32.const 3
                          i32.const 4
                          local.get 12
                          i32.const 65536
                          i32.lt_u
                          select
                          local.set 10
                        end
                        local.get 10
                        local.get 8
                        i32.add
                        local.set 5
                        br 3 (;@7;)
                      end
                      i32.const 3
                      local.set 13
                      i32.const 117
                      local.set 5
                      i32.const 3
                      local.set 10
                    end
                    local.get 2
                    i32.load offset=24
                    local.get 5
                    local.get 2
                    i32.load offset=28
                    i32.load offset=16
                    call_indirect (type 2)
                    br_if 2 (;@6;)
                    br 0 (;@8;)
                  end
                end
                local.get 8
                local.get 9
                i32.sub
                local.get 7
                i32.add
                local.set 8
                local.get 6
                local.get 7
                i32.ne
                br_if 1 (;@5;)
                br 2 (;@4;)
              end
            end
            i32.const 1
            local.set 4
            br 2 (;@2;)
          end
          local.get 5
          i32.eqz
          br_if 0 (;@3;)
          local.get 5
          local.get 1
          i32.eq
          br_if 0 (;@3;)
          local.get 5
          local.get 1
          i32.ge_u
          br_if 2 (;@1;)
          local.get 0
          local.get 5
          i32.add
          i32.load8_s
          i32.const -65
          i32.le_s
          br_if 2 (;@1;)
        end
        local.get 2
        i32.load offset=24
        local.get 0
        local.get 5
        i32.add
        local.get 1
        local.get 5
        i32.sub
        local.get 2
        i32.load offset=28
        i32.load offset=12
        call_indirect (type 6)
        br_if 0 (;@2;)
        local.get 2
        i32.load offset=24
        i32.const 34
        local.get 2
        i32.load offset=28
        i32.load offset=16
        call_indirect (type 2)
        local.set 4
      end
      local.get 3
      i32.const 48
      i32.add
      global.set 0
      local.get 4
      return
    end
    local.get 0
    local.get 1
    local.get 5
    local.get 1
    call $_ZN4core3str16slice_error_fail17ha06f3354b25aeac4E
    unreachable)
  (func $_ZN4core3str6traits101_$LT$impl$u20$core..slice..SliceIndex$LT$str$GT$$u20$for$u20$core..ops..range..Range$LT$usize$GT$$GT$5index28_$u7b$$u7b$closure$u7d$$u7d$17h019af69a3de9894cE (type 0) (param i32)
    (local i32)
    local.get 0
    i32.load
    local.tee 1
    i32.load
    local.get 1
    i32.load offset=4
    local.get 0
    i32.load offset=4
    i32.load
    local.get 0
    i32.load offset=8
    i32.load
    call $_ZN4core3str16slice_error_fail17ha06f3354b25aeac4E
    unreachable)
  (func $_ZN42_$LT$str$u20$as$u20$core..fmt..Display$GT$3fmt17hcf977a4d08f25cc0E (type 6) (param i32 i32 i32) (result i32)
    local.get 2
    local.get 0
    local.get 1
    call $_ZN4core3fmt9Formatter3pad17h7b301e85900e29c6E)
  (func $_ZN41_$LT$char$u20$as$u20$core..fmt..Debug$GT$3fmt17he8ac9a372b840f2eE (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    i32.const 1
    local.set 3
    block  ;; label = @1
      local.get 1
      i32.load offset=24
      i32.const 39
      local.get 1
      i32.const 28
      i32.add
      i32.load
      i32.load offset=16
      call_indirect (type 2)
      br_if 0 (;@1;)
      local.get 2
      local.get 0
      i32.load
      i32.const 1
      call $_ZN4core4char7methods22_$LT$impl$u20$char$GT$16escape_debug_ext17hb85592115fdc6803E
      local.get 2
      i32.const 12
      i32.add
      i32.load8_u
      local.set 4
      local.get 2
      i32.const 8
      i32.add
      i32.load
      local.set 5
      local.get 2
      i32.load
      local.set 3
      local.get 2
      i32.load offset=4
      local.tee 6
      i32.const 1114112
      i32.ne
      local.set 7
      loop  ;; label = @2
        local.get 3
        local.set 8
        i32.const 92
        local.set 0
        i32.const 1
        local.set 3
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  local.get 8
                  br_table 2 (;@5;) 1 (;@6;) 4 (;@3;) 0 (;@7;) 2 (;@5;)
                end
                local.get 4
                i32.const 255
                i32.and
                local.set 8
                i32.const 4
                local.set 4
                i32.const 3
                local.set 3
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      local.get 8
                      br_table 4 (;@5;) 2 (;@7;) 1 (;@8;) 0 (;@9;) 5 (;@4;) 6 (;@3;) 4 (;@5;)
                    end
                    i32.const 2
                    local.set 4
                    i32.const 123
                    local.set 0
                    br 5 (;@3;)
                  end
                  local.get 6
                  local.get 5
                  i32.const 2
                  i32.shl
                  i32.const 28
                  i32.and
                  i32.shr_u
                  i32.const 15
                  i32.and
                  local.tee 0
                  i32.const 48
                  i32.or
                  local.get 0
                  i32.const 87
                  i32.add
                  local.get 0
                  i32.const 10
                  i32.lt_u
                  select
                  local.set 0
                  i32.const 2
                  i32.const 1
                  local.get 5
                  select
                  local.set 4
                  local.get 5
                  i32.const -1
                  i32.add
                  i32.const 0
                  local.get 5
                  select
                  local.set 5
                  br 4 (;@3;)
                end
                i32.const 0
                local.set 4
                i32.const 125
                local.set 0
                br 3 (;@3;)
              end
              i32.const 0
              local.set 3
              local.get 6
              local.set 0
              local.get 7
              br_if 2 (;@3;)
            end
            local.get 1
            i32.load offset=24
            i32.const 39
            local.get 1
            i32.load offset=28
            i32.load offset=16
            call_indirect (type 2)
            local.set 3
            br 3 (;@1;)
          end
          i32.const 3
          local.set 3
          i32.const 117
          local.set 0
          i32.const 3
          local.set 4
        end
        local.get 1
        i32.load offset=24
        local.get 0
        local.get 1
        i32.load offset=28
        i32.load offset=16
        call_indirect (type 2)
        i32.eqz
        br_if 0 (;@2;)
      end
      i32.const 1
      local.set 3
    end
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    local.get 3)
  (func $_ZN4core3fmt3num53_$LT$impl$u20$core..fmt..LowerHex$u20$for$u20$i32$GT$3fmt17h957181898b1e70adE (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 128
    i32.sub
    local.tee 2
    global.set 0
    local.get 0
    i32.load
    local.set 3
    i32.const 0
    local.set 0
    loop  ;; label = @1
      local.get 2
      local.get 0
      i32.add
      i32.const 127
      i32.add
      local.get 3
      i32.const 15
      i32.and
      local.tee 4
      i32.const 48
      i32.or
      local.get 4
      i32.const 87
      i32.add
      local.get 4
      i32.const 10
      i32.lt_u
      select
      i32.store8
      local.get 0
      i32.const -1
      i32.add
      local.set 0
      local.get 3
      i32.const 4
      i32.shr_u
      local.tee 3
      br_if 0 (;@1;)
    end
    block  ;; label = @1
      local.get 0
      i32.const 128
      i32.add
      local.tee 3
      i32.const 129
      i32.lt_u
      br_if 0 (;@1;)
      local.get 3
      i32.const 128
      call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
      unreachable
    end
    local.get 1
    i32.const 1
    i32.const 1054629
    i32.const 2
    local.get 2
    local.get 0
    i32.add
    i32.const 128
    i32.add
    i32.const 0
    local.get 0
    i32.sub
    call $_ZN4core3fmt9Formatter12pad_integral17he52ae3771fdc9ec7E
    local.set 0
    local.get 2
    i32.const 128
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN4core3str5lossy9Utf8Lossy10from_bytes17h1357f46792efee29E (type 5) (param i32 i32 i32)
    local.get 0
    local.get 2
    i32.store offset=4
    local.get 0
    local.get 1
    i32.store)
  (func $_ZN4core3str5lossy9Utf8Lossy6chunks17hc5734690a50ae00eE (type 5) (param i32 i32 i32)
    local.get 0
    local.get 2
    i32.store offset=4
    local.get 0
    local.get 1
    i32.store)
  (func $_ZN96_$LT$core..str..lossy..Utf8LossyChunksIter$u20$as$u20$core..iter..traits..iterator..Iterator$GT$4next17hdd79ec53ab0551bdE (type 4) (param i32 i32)
    (local i32 i32 i32 i32 i32 i32 i32 i32 i32)
    block  ;; label = @1
      local.get 1
      i32.load offset=4
      local.tee 2
      i32.eqz
      br_if 0 (;@1;)
      local.get 1
      i32.load
      local.set 3
      i32.const 0
      local.set 4
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      block  ;; label = @10
                        block  ;; label = @11
                          block  ;; label = @12
                            block  ;; label = @13
                              loop  ;; label = @14
                                local.get 4
                                i32.const 1
                                i32.add
                                local.set 5
                                block  ;; label = @15
                                  block  ;; label = @16
                                    local.get 3
                                    local.get 4
                                    i32.add
                                    local.tee 6
                                    i32.load8_u
                                    local.tee 7
                                    i32.const 24
                                    i32.shl
                                    i32.const 24
                                    i32.shr_s
                                    local.tee 8
                                    i32.const -1
                                    i32.le_s
                                    br_if 0 (;@16;)
                                    local.get 5
                                    local.set 4
                                    br 1 (;@15;)
                                  end
                                  block  ;; label = @16
                                    block  ;; label = @17
                                      block  ;; label = @18
                                        block  ;; label = @19
                                          local.get 7
                                          i32.const 1055174
                                          i32.add
                                          i32.load8_u
                                          i32.const -2
                                          i32.add
                                          local.tee 9
                                          i32.const 2
                                          i32.gt_u
                                          br_if 0 (;@19;)
                                          local.get 9
                                          br_table 1 (;@18;) 2 (;@17;) 3 (;@16;) 1 (;@18;)
                                        end
                                        local.get 2
                                        local.get 4
                                        i32.lt_u
                                        br_if 15 (;@3;)
                                        local.get 2
                                        local.get 4
                                        i32.le_u
                                        br_if 16 (;@2;)
                                        local.get 0
                                        local.get 4
                                        i32.store offset=4
                                        local.get 0
                                        local.get 3
                                        i32.store
                                        local.get 1
                                        local.get 2
                                        local.get 5
                                        i32.sub
                                        i32.store offset=4
                                        local.get 1
                                        local.get 3
                                        local.get 5
                                        i32.add
                                        i32.store
                                        local.get 0
                                        i32.const 12
                                        i32.add
                                        i32.const 1
                                        i32.store
                                        local.get 0
                                        i32.const 8
                                        i32.add
                                        local.get 6
                                        i32.store
                                        return
                                      end
                                      block  ;; label = @18
                                        local.get 3
                                        local.get 5
                                        i32.add
                                        local.tee 8
                                        i32.const 0
                                        local.get 2
                                        local.get 5
                                        i32.gt_u
                                        select
                                        local.tee 7
                                        i32.const 1054353
                                        local.get 7
                                        select
                                        i32.load8_u
                                        i32.const 192
                                        i32.and
                                        i32.const 128
                                        i32.ne
                                        br_if 0 (;@18;)
                                        local.get 4
                                        i32.const 2
                                        i32.add
                                        local.set 4
                                        br 3 (;@15;)
                                      end
                                      local.get 2
                                      local.get 4
                                      i32.lt_u
                                      br_if 14 (;@3;)
                                      local.get 2
                                      local.get 4
                                      i32.le_u
                                      br_if 15 (;@2;)
                                      local.get 1
                                      local.get 8
                                      i32.store
                                      local.get 0
                                      local.get 4
                                      i32.store offset=4
                                      local.get 0
                                      local.get 3
                                      i32.store
                                      local.get 1
                                      local.get 2
                                      local.get 5
                                      i32.sub
                                      i32.store offset=4
                                      local.get 0
                                      i32.const 12
                                      i32.add
                                      i32.const 1
                                      i32.store
                                      local.get 0
                                      i32.const 8
                                      i32.add
                                      local.get 6
                                      i32.store
                                      return
                                    end
                                    local.get 3
                                    local.get 5
                                    i32.add
                                    local.tee 10
                                    i32.const 0
                                    local.get 2
                                    local.get 5
                                    i32.gt_u
                                    select
                                    local.tee 9
                                    i32.const 1054353
                                    local.get 9
                                    select
                                    i32.load8_u
                                    local.set 9
                                    block  ;; label = @17
                                      block  ;; label = @18
                                        local.get 7
                                        i32.const -224
                                        i32.add
                                        local.tee 7
                                        i32.const 13
                                        i32.gt_u
                                        br_if 0 (;@18;)
                                        block  ;; label = @19
                                          block  ;; label = @20
                                            local.get 7
                                            br_table 0 (;@20;) 2 (;@18;) 2 (;@18;) 2 (;@18;) 2 (;@18;) 2 (;@18;) 2 (;@18;) 2 (;@18;) 2 (;@18;) 2 (;@18;) 2 (;@18;) 2 (;@18;) 2 (;@18;) 1 (;@19;) 0 (;@20;)
                                          end
                                          local.get 9
                                          i32.const 224
                                          i32.and
                                          i32.const 160
                                          i32.eq
                                          br_if 2 (;@17;)
                                          br 15 (;@4;)
                                        end
                                        local.get 9
                                        i32.const 24
                                        i32.shl
                                        i32.const 24
                                        i32.shr_s
                                        i32.const -1
                                        i32.gt_s
                                        br_if 14 (;@4;)
                                        local.get 9
                                        i32.const 255
                                        i32.and
                                        i32.const 160
                                        i32.ge_u
                                        br_if 14 (;@4;)
                                        br 1 (;@17;)
                                      end
                                      block  ;; label = @18
                                        local.get 8
                                        i32.const 31
                                        i32.add
                                        i32.const 255
                                        i32.and
                                        i32.const 11
                                        i32.gt_u
                                        br_if 0 (;@18;)
                                        local.get 9
                                        i32.const 24
                                        i32.shl
                                        i32.const 24
                                        i32.shr_s
                                        i32.const -1
                                        i32.gt_s
                                        br_if 14 (;@4;)
                                        local.get 9
                                        i32.const 255
                                        i32.and
                                        i32.const 192
                                        i32.ge_u
                                        br_if 14 (;@4;)
                                        br 1 (;@17;)
                                      end
                                      local.get 9
                                      i32.const 255
                                      i32.and
                                      i32.const 191
                                      i32.gt_u
                                      br_if 13 (;@4;)
                                      local.get 8
                                      i32.const 254
                                      i32.and
                                      i32.const 238
                                      i32.ne
                                      br_if 13 (;@4;)
                                      local.get 9
                                      i32.const 24
                                      i32.shl
                                      i32.const 24
                                      i32.shr_s
                                      i32.const -1
                                      i32.gt_s
                                      br_if 13 (;@4;)
                                    end
                                    block  ;; label = @17
                                      local.get 3
                                      local.get 4
                                      i32.const 2
                                      i32.add
                                      local.tee 5
                                      i32.add
                                      local.tee 8
                                      i32.const 0
                                      local.get 2
                                      local.get 5
                                      i32.gt_u
                                      select
                                      local.tee 7
                                      i32.const 1054353
                                      local.get 7
                                      select
                                      i32.load8_u
                                      i32.const 192
                                      i32.and
                                      i32.const 128
                                      i32.ne
                                      br_if 0 (;@17;)
                                      local.get 4
                                      i32.const 3
                                      i32.add
                                      local.set 4
                                      br 2 (;@15;)
                                    end
                                    local.get 2
                                    local.get 4
                                    i32.lt_u
                                    br_if 13 (;@3;)
                                    local.get 4
                                    i32.const -3
                                    i32.gt_u
                                    br_if 5 (;@11;)
                                    local.get 2
                                    local.get 5
                                    i32.lt_u
                                    br_if 6 (;@10;)
                                    local.get 1
                                    local.get 8
                                    i32.store
                                    local.get 0
                                    local.get 4
                                    i32.store offset=4
                                    local.get 0
                                    local.get 3
                                    i32.store
                                    local.get 1
                                    local.get 2
                                    local.get 5
                                    i32.sub
                                    i32.store offset=4
                                    local.get 0
                                    i32.const 12
                                    i32.add
                                    i32.const 2
                                    i32.store
                                    local.get 0
                                    i32.const 8
                                    i32.add
                                    local.get 6
                                    i32.store
                                    return
                                  end
                                  local.get 3
                                  local.get 5
                                  i32.add
                                  local.tee 10
                                  i32.const 0
                                  local.get 2
                                  local.get 5
                                  i32.gt_u
                                  select
                                  local.tee 9
                                  i32.const 1054353
                                  local.get 9
                                  select
                                  i32.load8_u
                                  local.set 9
                                  block  ;; label = @16
                                    block  ;; label = @17
                                      local.get 7
                                      i32.const -240
                                      i32.add
                                      local.tee 7
                                      i32.const 4
                                      i32.gt_u
                                      br_if 0 (;@17;)
                                      block  ;; label = @18
                                        block  ;; label = @19
                                          local.get 7
                                          br_table 0 (;@19;) 2 (;@17;) 2 (;@17;) 2 (;@17;) 1 (;@18;) 0 (;@19;)
                                        end
                                        local.get 9
                                        i32.const 112
                                        i32.add
                                        i32.const 255
                                        i32.and
                                        i32.const 48
                                        i32.lt_u
                                        br_if 2 (;@16;)
                                        br 13 (;@5;)
                                      end
                                      local.get 9
                                      i32.const 24
                                      i32.shl
                                      i32.const 24
                                      i32.shr_s
                                      i32.const -1
                                      i32.gt_s
                                      br_if 12 (;@5;)
                                      local.get 9
                                      i32.const 255
                                      i32.and
                                      i32.const 144
                                      i32.ge_u
                                      br_if 12 (;@5;)
                                      br 1 (;@16;)
                                    end
                                    local.get 9
                                    i32.const 255
                                    i32.and
                                    i32.const 191
                                    i32.gt_u
                                    br_if 11 (;@5;)
                                    local.get 8
                                    i32.const 15
                                    i32.add
                                    i32.const 255
                                    i32.and
                                    i32.const 2
                                    i32.gt_u
                                    br_if 11 (;@5;)
                                    local.get 9
                                    i32.const 24
                                    i32.shl
                                    i32.const 24
                                    i32.shr_s
                                    i32.const -1
                                    i32.gt_s
                                    br_if 11 (;@5;)
                                  end
                                  local.get 3
                                  local.get 4
                                  i32.const 2
                                  i32.add
                                  local.tee 5
                                  i32.add
                                  local.tee 8
                                  i32.const 0
                                  local.get 2
                                  local.get 5
                                  i32.gt_u
                                  select
                                  local.tee 7
                                  i32.const 1054353
                                  local.get 7
                                  select
                                  i32.load8_u
                                  i32.const 192
                                  i32.and
                                  i32.const 128
                                  i32.ne
                                  br_if 2 (;@13;)
                                  local.get 3
                                  local.get 4
                                  i32.const 3
                                  i32.add
                                  local.tee 5
                                  i32.add
                                  local.tee 8
                                  i32.const 0
                                  local.get 2
                                  local.get 5
                                  i32.gt_u
                                  select
                                  local.tee 7
                                  i32.const 1054353
                                  local.get 7
                                  select
                                  i32.load8_u
                                  i32.const 192
                                  i32.and
                                  i32.const 128
                                  i32.ne
                                  br_if 3 (;@12;)
                                  local.get 4
                                  i32.const 4
                                  i32.add
                                  local.set 4
                                end
                                local.get 4
                                local.get 2
                                i32.lt_u
                                br_if 0 (;@14;)
                              end
                              local.get 1
                              i32.const 0
                              i32.store offset=4
                              local.get 1
                              i32.const 1054352
                              i32.store
                              local.get 0
                              local.get 2
                              i32.store offset=4
                              local.get 0
                              local.get 3
                              i32.store
                              local.get 0
                              i32.const 12
                              i32.add
                              i32.const 0
                              i32.store
                              local.get 0
                              i32.const 8
                              i32.add
                              i32.const 1054352
                              i32.store
                              return
                            end
                            local.get 2
                            local.get 4
                            i32.lt_u
                            br_if 9 (;@3;)
                            local.get 4
                            i32.const -3
                            i32.gt_u
                            br_if 3 (;@9;)
                            local.get 2
                            local.get 5
                            i32.lt_u
                            br_if 4 (;@8;)
                            local.get 1
                            local.get 8
                            i32.store
                            local.get 0
                            local.get 4
                            i32.store offset=4
                            local.get 0
                            local.get 3
                            i32.store
                            local.get 1
                            local.get 2
                            local.get 5
                            i32.sub
                            i32.store offset=4
                            local.get 0
                            i32.const 12
                            i32.add
                            i32.const 2
                            i32.store
                            local.get 0
                            i32.const 8
                            i32.add
                            local.get 6
                            i32.store
                            return
                          end
                          local.get 2
                          local.get 4
                          i32.lt_u
                          br_if 8 (;@3;)
                          local.get 4
                          i32.const -4
                          i32.gt_u
                          br_if 4 (;@7;)
                          local.get 2
                          local.get 5
                          i32.lt_u
                          br_if 5 (;@6;)
                          local.get 1
                          local.get 8
                          i32.store
                          local.get 0
                          local.get 4
                          i32.store offset=4
                          local.get 0
                          local.get 3
                          i32.store
                          local.get 1
                          local.get 2
                          local.get 5
                          i32.sub
                          i32.store offset=4
                          local.get 0
                          i32.const 12
                          i32.add
                          i32.const 3
                          i32.store
                          local.get 0
                          i32.const 8
                          i32.add
                          local.get 6
                          i32.store
                          return
                        end
                        local.get 4
                        local.get 5
                        call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
                        unreachable
                      end
                      local.get 5
                      local.get 2
                      call $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E
                      unreachable
                    end
                    local.get 4
                    local.get 5
                    call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
                    unreachable
                  end
                  local.get 5
                  local.get 2
                  call $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E
                  unreachable
                end
                local.get 4
                local.get 5
                call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
                unreachable
              end
              local.get 5
              local.get 2
              call $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E
              unreachable
            end
            local.get 2
            local.get 4
            i32.lt_u
            br_if 1 (;@3;)
            local.get 2
            local.get 4
            i32.le_u
            br_if 2 (;@2;)
            local.get 1
            local.get 10
            i32.store
            local.get 0
            local.get 4
            i32.store offset=4
            local.get 0
            local.get 3
            i32.store
            local.get 1
            local.get 2
            local.get 5
            i32.sub
            i32.store offset=4
            local.get 0
            i32.const 12
            i32.add
            i32.const 1
            i32.store
            local.get 0
            i32.const 8
            i32.add
            local.get 6
            i32.store
            return
          end
          local.get 2
          local.get 4
          i32.lt_u
          br_if 0 (;@3;)
          local.get 2
          local.get 4
          i32.le_u
          br_if 1 (;@2;)
          local.get 1
          local.get 10
          i32.store
          local.get 0
          local.get 4
          i32.store offset=4
          local.get 0
          local.get 3
          i32.store
          local.get 1
          local.get 2
          local.get 5
          i32.sub
          i32.store offset=4
          local.get 0
          i32.const 12
          i32.add
          i32.const 1
          i32.store
          local.get 0
          i32.const 8
          i32.add
          local.get 6
          i32.store
          return
        end
        local.get 4
        local.get 2
        call $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E
        unreachable
      end
      local.get 5
      local.get 2
      call $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E
      unreachable
    end
    local.get 0
    i32.const 0
    i32.store)
  (func $_ZN66_$LT$core..str..lossy..Utf8Lossy$u20$as$u20$core..fmt..Display$GT$3fmt17haeaf8cd40e5faeccE (type 6) (param i32 i32 i32) (result i32)
    (local i32 i32 i32 i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee 3
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 1
          i32.eqz
          br_if 0 (;@3;)
          local.get 3
          local.get 1
          i32.store offset=12
          local.get 3
          local.get 0
          i32.store offset=8
          local.get 3
          i32.const 16
          i32.add
          local.get 3
          i32.const 8
          i32.add
          call $_ZN96_$LT$core..str..lossy..Utf8LossyChunksIter$u20$as$u20$core..iter..traits..iterator..Iterator$GT$4next17hdd79ec53ab0551bdE
          block  ;; label = @4
            local.get 3
            i32.load offset=16
            local.tee 0
            i32.eqz
            br_if 0 (;@4;)
            loop  ;; label = @5
              local.get 3
              i32.load offset=28
              local.set 4
              local.get 3
              i32.load offset=20
              local.tee 5
              local.get 1
              i32.eq
              br_if 3 (;@2;)
              i32.const 1
              local.set 6
              local.get 2
              i32.load offset=24
              local.get 0
              local.get 5
              local.get 2
              i32.load offset=28
              i32.load offset=12
              call_indirect (type 6)
              br_if 4 (;@1;)
              block  ;; label = @6
                local.get 4
                i32.eqz
                br_if 0 (;@6;)
                local.get 2
                i32.load offset=24
                i32.const 65533
                local.get 2
                i32.load offset=28
                i32.load offset=16
                call_indirect (type 2)
                br_if 5 (;@1;)
              end
              local.get 3
              i32.const 16
              i32.add
              local.get 3
              i32.const 8
              i32.add
              call $_ZN96_$LT$core..str..lossy..Utf8LossyChunksIter$u20$as$u20$core..iter..traits..iterator..Iterator$GT$4next17hdd79ec53ab0551bdE
              local.get 3
              i32.load offset=16
              local.tee 0
              br_if 0 (;@5;)
            end
          end
          i32.const 0
          local.set 6
          br 2 (;@1;)
        end
        local.get 2
        i32.const 1054352
        i32.const 0
        call $_ZN4core3fmt9Formatter3pad17h7b301e85900e29c6E
        local.set 6
        br 1 (;@1;)
      end
      block  ;; label = @2
        local.get 4
        br_if 0 (;@2;)
        local.get 2
        local.get 0
        local.get 1
        call $_ZN4core3fmt9Formatter3pad17h7b301e85900e29c6E
        local.set 6
        br 1 (;@1;)
      end
      i32.const 1055076
      i32.const 35
      i32.const 1055136
      call $_ZN4core9panicking5panic17he9463ceb3e2615beE
      unreachable
    end
    local.get 3
    i32.const 32
    i32.add
    global.set 0
    local.get 6)
  (func $_ZN4core3fmt3num52_$LT$impl$u20$core..fmt..LowerHex$u20$for$u20$i8$GT$3fmt17hee01ea12b036c189E (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 128
    i32.sub
    local.tee 2
    global.set 0
    local.get 0
    i32.load8_u
    local.set 3
    i32.const 0
    local.set 0
    loop  ;; label = @1
      local.get 2
      local.get 0
      i32.add
      i32.const 127
      i32.add
      local.get 3
      i32.const 15
      i32.and
      local.tee 4
      i32.const 48
      i32.or
      local.get 4
      i32.const 87
      i32.add
      local.get 4
      i32.const 10
      i32.lt_u
      select
      i32.store8
      local.get 0
      i32.const -1
      i32.add
      local.set 0
      local.get 3
      i32.const 4
      i32.shr_u
      i32.const 15
      i32.and
      local.tee 3
      br_if 0 (;@1;)
    end
    block  ;; label = @1
      local.get 0
      i32.const 128
      i32.add
      local.tee 3
      i32.const 129
      i32.lt_u
      br_if 0 (;@1;)
      local.get 3
      i32.const 128
      call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
      unreachable
    end
    local.get 1
    i32.const 1
    i32.const 1054629
    i32.const 2
    local.get 2
    local.get 0
    i32.add
    i32.const 128
    i32.add
    i32.const 0
    local.get 0
    i32.sub
    call $_ZN4core3fmt9Formatter12pad_integral17he52ae3771fdc9ec7E
    local.set 0
    local.get 2
    i32.const 128
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN4core3str9from_utf817h40c83401242cc090E (type 5) (param i32 i32 i32)
    (local i32 i64)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 3
    global.set 0
    local.get 3
    i32.const 8
    i32.add
    local.get 1
    local.get 2
    call $_ZN4core3str19run_utf8_validation17h0998bfd07ea5db30E
    block  ;; label = @1
      block  ;; label = @2
        local.get 3
        i64.load offset=8
        local.tee 4
        i64.const 1095216660480
        i64.and
        i64.const 8589934592
        i64.eq
        br_if 0 (;@2;)
        local.get 0
        local.get 4
        i64.store offset=4 align=4
        i32.const 1
        local.set 1
        br 1 (;@1;)
      end
      local.get 0
      local.get 1
      i32.store offset=4
      local.get 0
      i32.const 8
      i32.add
      local.get 2
      i32.store
      i32.const 0
      local.set 1
    end
    local.get 0
    local.get 1
    i32.store
    local.get 3
    i32.const 16
    i32.add
    global.set 0)
  (func $_ZN4core3str19run_utf8_validation17h0998bfd07ea5db30E (type 5) (param i32 i32 i32)
    (local i32 i32 i32 i32 i32 i32)
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 2
          i32.eqz
          br_if 0 (;@3;)
          i32.const 0
          local.get 1
          i32.sub
          i32.const 0
          local.get 1
          i32.const 3
          i32.and
          select
          local.set 3
          local.get 2
          i32.const -7
          i32.add
          i32.const 0
          local.get 2
          i32.const 7
          i32.gt_u
          select
          local.set 4
          i32.const 0
          local.set 5
          loop  ;; label = @4
            block  ;; label = @5
              block  ;; label = @6
                block  ;; label = @7
                  block  ;; label = @8
                    block  ;; label = @9
                      local.get 1
                      local.get 5
                      i32.add
                      i32.load8_u
                      local.tee 6
                      i32.const 24
                      i32.shl
                      i32.const 24
                      i32.shr_s
                      local.tee 7
                      i32.const -1
                      i32.gt_s
                      br_if 0 (;@9;)
                      block  ;; label = @10
                        block  ;; label = @11
                          block  ;; label = @12
                            block  ;; label = @13
                              local.get 6
                              i32.const 1055174
                              i32.add
                              i32.load8_u
                              i32.const -2
                              i32.add
                              local.tee 8
                              i32.const 2
                              i32.gt_u
                              br_if 0 (;@13;)
                              local.get 8
                              br_table 1 (;@12;) 2 (;@11;) 3 (;@10;) 1 (;@12;)
                            end
                            local.get 0
                            i32.const 257
                            i32.store16 offset=4
                            local.get 0
                            local.get 5
                            i32.store
                            return
                          end
                          block  ;; label = @12
                            local.get 5
                            i32.const 1
                            i32.add
                            local.tee 6
                            local.get 2
                            i32.lt_u
                            br_if 0 (;@12;)
                            local.get 0
                            i32.const 0
                            i32.store8 offset=4
                            local.get 0
                            local.get 5
                            i32.store
                            return
                          end
                          local.get 1
                          local.get 6
                          i32.add
                          i32.load8_u
                          i32.const 192
                          i32.and
                          i32.const 128
                          i32.eq
                          br_if 3 (;@8;)
                          local.get 0
                          i32.const 257
                          i32.store16 offset=4
                          local.get 0
                          local.get 5
                          i32.store
                          return
                        end
                        block  ;; label = @11
                          local.get 5
                          i32.const 1
                          i32.add
                          local.tee 8
                          local.get 2
                          i32.lt_u
                          br_if 0 (;@11;)
                          local.get 0
                          i32.const 0
                          i32.store8 offset=4
                          local.get 0
                          local.get 5
                          i32.store
                          return
                        end
                        local.get 1
                        local.get 8
                        i32.add
                        i32.load8_u
                        local.set 8
                        block  ;; label = @11
                          block  ;; label = @12
                            local.get 6
                            i32.const -224
                            i32.add
                            local.tee 6
                            i32.const 13
                            i32.gt_u
                            br_if 0 (;@12;)
                            block  ;; label = @13
                              block  ;; label = @14
                                local.get 6
                                br_table 0 (;@14;) 2 (;@12;) 2 (;@12;) 2 (;@12;) 2 (;@12;) 2 (;@12;) 2 (;@12;) 2 (;@12;) 2 (;@12;) 2 (;@12;) 2 (;@12;) 2 (;@12;) 2 (;@12;) 1 (;@13;) 0 (;@14;)
                              end
                              local.get 8
                              i32.const 224
                              i32.and
                              i32.const 160
                              i32.ne
                              br_if 12 (;@1;)
                              br 2 (;@11;)
                            end
                            local.get 8
                            i32.const 24
                            i32.shl
                            i32.const 24
                            i32.shr_s
                            i32.const -1
                            i32.gt_s
                            br_if 11 (;@1;)
                            local.get 8
                            i32.const 255
                            i32.and
                            i32.const 160
                            i32.lt_u
                            br_if 1 (;@11;)
                            br 11 (;@1;)
                          end
                          block  ;; label = @12
                            local.get 7
                            i32.const 31
                            i32.add
                            i32.const 255
                            i32.and
                            i32.const 11
                            i32.gt_u
                            br_if 0 (;@12;)
                            local.get 8
                            i32.const 24
                            i32.shl
                            i32.const 24
                            i32.shr_s
                            i32.const -1
                            i32.gt_s
                            br_if 11 (;@1;)
                            local.get 8
                            i32.const 255
                            i32.and
                            i32.const 192
                            i32.ge_u
                            br_if 11 (;@1;)
                            br 1 (;@11;)
                          end
                          local.get 8
                          i32.const 255
                          i32.and
                          i32.const 191
                          i32.gt_u
                          br_if 10 (;@1;)
                          local.get 7
                          i32.const 254
                          i32.and
                          i32.const 238
                          i32.ne
                          br_if 10 (;@1;)
                          local.get 8
                          i32.const 24
                          i32.shl
                          i32.const 24
                          i32.shr_s
                          i32.const -1
                          i32.gt_s
                          br_if 10 (;@1;)
                        end
                        block  ;; label = @11
                          local.get 5
                          i32.const 2
                          i32.add
                          local.tee 6
                          local.get 2
                          i32.lt_u
                          br_if 0 (;@11;)
                          local.get 0
                          i32.const 0
                          i32.store8 offset=4
                          local.get 0
                          local.get 5
                          i32.store
                          return
                        end
                        local.get 1
                        local.get 6
                        i32.add
                        i32.load8_u
                        i32.const 192
                        i32.and
                        i32.const 128
                        i32.eq
                        br_if 2 (;@8;)
                        local.get 0
                        i32.const 513
                        i32.store16 offset=4
                        local.get 0
                        local.get 5
                        i32.store
                        return
                      end
                      block  ;; label = @10
                        local.get 5
                        i32.const 1
                        i32.add
                        local.tee 8
                        local.get 2
                        i32.lt_u
                        br_if 0 (;@10;)
                        local.get 0
                        i32.const 0
                        i32.store8 offset=4
                        local.get 0
                        local.get 5
                        i32.store
                        return
                      end
                      local.get 1
                      local.get 8
                      i32.add
                      i32.load8_u
                      local.set 8
                      block  ;; label = @10
                        block  ;; label = @11
                          local.get 6
                          i32.const -240
                          i32.add
                          local.tee 6
                          i32.const 4
                          i32.gt_u
                          br_if 0 (;@11;)
                          block  ;; label = @12
                            block  ;; label = @13
                              local.get 6
                              br_table 0 (;@13;) 2 (;@11;) 2 (;@11;) 2 (;@11;) 1 (;@12;) 0 (;@13;)
                            end
                            local.get 8
                            i32.const 112
                            i32.add
                            i32.const 255
                            i32.and
                            i32.const 48
                            i32.ge_u
                            br_if 10 (;@2;)
                            br 2 (;@10;)
                          end
                          local.get 8
                          i32.const 24
                          i32.shl
                          i32.const 24
                          i32.shr_s
                          i32.const -1
                          i32.gt_s
                          br_if 9 (;@2;)
                          local.get 8
                          i32.const 255
                          i32.and
                          i32.const 144
                          i32.lt_u
                          br_if 1 (;@10;)
                          br 9 (;@2;)
                        end
                        local.get 8
                        i32.const 255
                        i32.and
                        i32.const 191
                        i32.gt_u
                        br_if 8 (;@2;)
                        local.get 7
                        i32.const 15
                        i32.add
                        i32.const 255
                        i32.and
                        i32.const 2
                        i32.gt_u
                        br_if 8 (;@2;)
                        local.get 8
                        i32.const 24
                        i32.shl
                        i32.const 24
                        i32.shr_s
                        i32.const -1
                        i32.gt_s
                        br_if 8 (;@2;)
                      end
                      block  ;; label = @10
                        local.get 5
                        i32.const 2
                        i32.add
                        local.tee 6
                        local.get 2
                        i32.lt_u
                        br_if 0 (;@10;)
                        local.get 0
                        i32.const 0
                        i32.store8 offset=4
                        local.get 0
                        local.get 5
                        i32.store
                        return
                      end
                      local.get 1
                      local.get 6
                      i32.add
                      i32.load8_u
                      i32.const 192
                      i32.and
                      i32.const 128
                      i32.ne
                      br_if 2 (;@7;)
                      block  ;; label = @10
                        local.get 5
                        i32.const 3
                        i32.add
                        local.tee 6
                        local.get 2
                        i32.lt_u
                        br_if 0 (;@10;)
                        local.get 0
                        i32.const 0
                        i32.store8 offset=4
                        local.get 0
                        local.get 5
                        i32.store
                        return
                      end
                      local.get 1
                      local.get 6
                      i32.add
                      i32.load8_u
                      i32.const 192
                      i32.and
                      i32.const 128
                      i32.eq
                      br_if 1 (;@8;)
                      local.get 0
                      i32.const 769
                      i32.store16 offset=4
                      local.get 0
                      local.get 5
                      i32.store
                      return
                    end
                    local.get 3
                    local.get 5
                    i32.sub
                    i32.const 3
                    i32.and
                    br_if 2 (;@6;)
                    block  ;; label = @9
                      local.get 5
                      local.get 4
                      i32.ge_u
                      br_if 0 (;@9;)
                      loop  ;; label = @10
                        local.get 1
                        local.get 5
                        i32.add
                        local.tee 6
                        i32.const 4
                        i32.add
                        i32.load
                        local.get 6
                        i32.load
                        i32.or
                        i32.const -2139062144
                        i32.and
                        br_if 1 (;@9;)
                        local.get 5
                        i32.const 8
                        i32.add
                        local.tee 5
                        local.get 4
                        i32.lt_u
                        br_if 0 (;@10;)
                      end
                    end
                    local.get 5
                    local.get 2
                    i32.ge_u
                    br_if 3 (;@5;)
                    loop  ;; label = @9
                      local.get 1
                      local.get 5
                      i32.add
                      i32.load8_s
                      i32.const 0
                      i32.lt_s
                      br_if 4 (;@5;)
                      local.get 2
                      local.get 5
                      i32.const 1
                      i32.add
                      local.tee 5
                      i32.ne
                      br_if 0 (;@9;)
                      br 6 (;@3;)
                    end
                  end
                  local.get 6
                  i32.const 1
                  i32.add
                  local.set 5
                  br 2 (;@5;)
                end
                local.get 0
                i32.const 513
                i32.store16 offset=4
                local.get 0
                local.get 5
                i32.store
                return
              end
              local.get 5
              i32.const 1
              i32.add
              local.set 5
            end
            local.get 5
            local.get 2
            i32.lt_u
            br_if 0 (;@4;)
          end
        end
        local.get 0
        i32.const 2
        i32.store8 offset=4
        return
      end
      local.get 0
      i32.const 257
      i32.store16 offset=4
      local.get 0
      local.get 5
      i32.store
      return
    end
    local.get 0
    i32.const 257
    i32.store16 offset=4
    local.get 0
    local.get 5
    i32.store)
  (func $_ZN4core3fmt3num3imp51_$LT$impl$u20$core..fmt..Display$u20$for$u20$u8$GT$3fmt17h041f3dd69513682bE (type 2) (param i32 i32) (result i32)
    local.get 0
    i64.load8_u
    i32.const 1
    local.get 1
    call $_ZN4core3fmt3num3imp7fmt_u6417h035a0daf9e6f2b5cE)
  (func $_ZN4core7unicode9printable5check17h32a85d89e686515aE (type 18) (param i32 i32 i32 i32 i32 i32 i32) (result i32)
    (local i32 i32 i32 i32 i32 i32 i32)
    i32.const 1
    local.set 7
    block  ;; label = @1
      block  ;; label = @2
        local.get 2
        i32.eqz
        br_if 0 (;@2;)
        local.get 1
        local.get 2
        i32.const 1
        i32.shl
        i32.add
        local.set 8
        local.get 0
        i32.const 65280
        i32.and
        i32.const 8
        i32.shr_u
        local.set 9
        i32.const 0
        local.set 10
        local.get 0
        i32.const 255
        i32.and
        local.set 11
        block  ;; label = @3
          loop  ;; label = @4
            local.get 1
            i32.const 2
            i32.add
            local.set 12
            local.get 10
            local.get 1
            i32.load8_u offset=1
            local.tee 2
            i32.add
            local.set 13
            block  ;; label = @5
              local.get 1
              i32.load8_u
              local.tee 1
              local.get 9
              i32.eq
              br_if 0 (;@5;)
              local.get 1
              local.get 9
              i32.gt_u
              br_if 3 (;@2;)
              local.get 13
              local.set 10
              local.get 12
              local.set 1
              local.get 12
              local.get 8
              i32.ne
              br_if 1 (;@4;)
              br 3 (;@2;)
            end
            block  ;; label = @5
              local.get 13
              local.get 10
              i32.lt_u
              br_if 0 (;@5;)
              local.get 13
              local.get 4
              i32.gt_u
              br_if 2 (;@3;)
              local.get 3
              local.get 10
              i32.add
              local.set 1
              block  ;; label = @6
                loop  ;; label = @7
                  local.get 2
                  i32.eqz
                  br_if 1 (;@6;)
                  local.get 2
                  i32.const -1
                  i32.add
                  local.set 2
                  local.get 1
                  i32.load8_u
                  local.set 10
                  local.get 1
                  i32.const 1
                  i32.add
                  local.set 1
                  local.get 10
                  local.get 11
                  i32.ne
                  br_if 0 (;@7;)
                end
                i32.const 0
                local.set 7
                br 5 (;@1;)
              end
              local.get 13
              local.set 10
              local.get 12
              local.set 1
              local.get 12
              local.get 8
              i32.ne
              br_if 1 (;@4;)
              br 3 (;@2;)
            end
          end
          local.get 10
          local.get 13
          call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
          unreachable
        end
        local.get 13
        local.get 4
        call $_ZN4core5slice20slice_index_len_fail17h84a3deeb0662a3e7E
        unreachable
      end
      local.get 6
      i32.eqz
      br_if 0 (;@1;)
      local.get 5
      local.get 6
      i32.add
      local.set 11
      local.get 0
      i32.const 65535
      i32.and
      local.set 1
      i32.const 1
      local.set 7
      block  ;; label = @2
        loop  ;; label = @3
          local.get 5
          i32.const 1
          i32.add
          local.set 10
          block  ;; label = @4
            block  ;; label = @5
              local.get 5
              i32.load8_u
              local.tee 2
              i32.const 24
              i32.shl
              i32.const 24
              i32.shr_s
              local.tee 13
              i32.const 0
              i32.lt_s
              br_if 0 (;@5;)
              local.get 10
              local.set 5
              br 1 (;@4;)
            end
            local.get 10
            local.get 11
            i32.eq
            br_if 2 (;@2;)
            local.get 13
            i32.const 127
            i32.and
            i32.const 8
            i32.shl
            local.get 5
            i32.load8_u offset=1
            i32.or
            local.set 2
            local.get 5
            i32.const 2
            i32.add
            local.set 5
          end
          local.get 1
          local.get 2
          i32.sub
          local.tee 1
          i32.const 0
          i32.lt_s
          br_if 2 (;@1;)
          local.get 7
          i32.const 1
          i32.xor
          local.set 7
          local.get 5
          local.get 11
          i32.ne
          br_if 0 (;@3;)
          br 2 (;@1;)
        end
      end
      i32.const 1054389
      i32.const 43
      i32.const 1055748
      call $_ZN4core9panicking5panic17he9463ceb3e2615beE
      unreachable
    end
    local.get 7
    i32.const 1
    i32.and)
  (func $_ZN4core3fmt3num52_$LT$impl$u20$core..fmt..UpperHex$u20$for$u20$i8$GT$3fmt17hebc0280df4365f65E (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 128
    i32.sub
    local.tee 2
    global.set 0
    local.get 0
    i32.load8_u
    local.set 3
    i32.const 0
    local.set 0
    loop  ;; label = @1
      local.get 2
      local.get 0
      i32.add
      i32.const 127
      i32.add
      local.get 3
      i32.const 15
      i32.and
      local.tee 4
      i32.const 48
      i32.or
      local.get 4
      i32.const 55
      i32.add
      local.get 4
      i32.const 10
      i32.lt_u
      select
      i32.store8
      local.get 0
      i32.const -1
      i32.add
      local.set 0
      local.get 3
      i32.const 4
      i32.shr_u
      i32.const 15
      i32.and
      local.tee 3
      br_if 0 (;@1;)
    end
    block  ;; label = @1
      local.get 0
      i32.const 128
      i32.add
      local.tee 3
      i32.const 129
      i32.lt_u
      br_if 0 (;@1;)
      local.get 3
      i32.const 128
      call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
      unreachable
    end
    local.get 1
    i32.const 1
    i32.const 1054629
    i32.const 2
    local.get 2
    local.get 0
    i32.add
    i32.const 128
    i32.add
    i32.const 0
    local.get 0
    i32.sub
    call $_ZN4core3fmt9Formatter12pad_integral17he52ae3771fdc9ec7E
    local.set 0
    local.get 2
    i32.const 128
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN4core3fmt3num53_$LT$impl$u20$core..fmt..UpperHex$u20$for$u20$i32$GT$3fmt17hb943c22d3c4cfecbE (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 128
    i32.sub
    local.tee 2
    global.set 0
    local.get 0
    i32.load
    local.set 3
    i32.const 0
    local.set 0
    loop  ;; label = @1
      local.get 2
      local.get 0
      i32.add
      i32.const 127
      i32.add
      local.get 3
      i32.const 15
      i32.and
      local.tee 4
      i32.const 48
      i32.or
      local.get 4
      i32.const 55
      i32.add
      local.get 4
      i32.const 10
      i32.lt_u
      select
      i32.store8
      local.get 0
      i32.const -1
      i32.add
      local.set 0
      local.get 3
      i32.const 4
      i32.shr_u
      local.tee 3
      br_if 0 (;@1;)
    end
    block  ;; label = @1
      local.get 0
      i32.const 128
      i32.add
      local.tee 3
      i32.const 129
      i32.lt_u
      br_if 0 (;@1;)
      local.get 3
      i32.const 128
      call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
      unreachable
    end
    local.get 1
    i32.const 1
    i32.const 1054629
    i32.const 2
    local.get 2
    local.get 0
    i32.add
    i32.const 128
    i32.add
    i32.const 0
    local.get 0
    i32.sub
    call $_ZN4core3fmt9Formatter12pad_integral17he52ae3771fdc9ec7E
    local.set 0
    local.get 2
    i32.const 128
    i32.add
    global.set 0
    local.get 0)
  (func $_ZN4core3fmt3num3imp52_$LT$impl$u20$core..fmt..Display$u20$for$u20$i32$GT$3fmt17hcfda14b3662191abE (type 2) (param i32 i32) (result i32)
    (local i64)
    local.get 0
    i32.load
    local.tee 0
    i64.extend_i32_s
    local.tee 2
    local.get 2
    i64.const 63
    i64.shr_s
    local.tee 2
    i64.add
    local.get 2
    i64.xor
    local.get 0
    i32.const -1
    i32.xor
    i32.const 31
    i32.shr_u
    local.get 1
    call $_ZN4core3fmt3num3imp7fmt_u6417h035a0daf9e6f2b5cE)
  (func $_ZN53_$LT$core..fmt..Error$u20$as$u20$core..fmt..Debug$GT$3fmt17h510f284fdbf86330E (type 2) (param i32 i32) (result i32)
    local.get 1
    i32.load offset=24
    i32.const 1057180
    i32.const 5
    local.get 1
    i32.const 28
    i32.add
    i32.load
    i32.load offset=12
    call_indirect (type 6))
  (func $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h8401558e7b8ffa27E (type 2) (param i32 i32) (result i32)
    local.get 0
    i32.load
    local.get 1
    call $_ZN4core3fmt3num52_$LT$impl$u20$core..fmt..Debug$u20$for$u20$usize$GT$3fmt17h65f5d7159ee75f4fE)
  (func $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17hb0114246c3496e11E (type 2) (param i32 i32) (result i32)
    (local i32 i32 i32)
    global.get 0
    i32.const 128
    i32.sub
    local.tee 2
    global.set 0
    local.get 0
    i32.load
    local.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            block  ;; label = @5
              local.get 1
              i32.load
              local.tee 3
              i32.const 16
              i32.and
              br_if 0 (;@5;)
              local.get 0
              i32.load8_u
              local.set 4
              local.get 3
              i32.const 32
              i32.and
              br_if 1 (;@4;)
              local.get 4
              i64.extend_i32_u
              i64.const 255
              i64.and
              i32.const 1
              local.get 1
              call $_ZN4core3fmt3num3imp7fmt_u6417h035a0daf9e6f2b5cE
              local.set 0
              br 2 (;@3;)
            end
            local.get 0
            i32.load8_u
            local.set 4
            i32.const 0
            local.set 0
            loop  ;; label = @5
              local.get 2
              local.get 0
              i32.add
              i32.const 127
              i32.add
              local.get 4
              i32.const 15
              i32.and
              local.tee 3
              i32.const 48
              i32.or
              local.get 3
              i32.const 87
              i32.add
              local.get 3
              i32.const 10
              i32.lt_u
              select
              i32.store8
              local.get 0
              i32.const -1
              i32.add
              local.set 0
              local.get 4
              i32.const 4
              i32.shr_u
              i32.const 15
              i32.and
              local.tee 4
              br_if 0 (;@5;)
            end
            local.get 0
            i32.const 128
            i32.add
            local.tee 4
            i32.const 129
            i32.ge_u
            br_if 2 (;@2;)
            local.get 1
            i32.const 1
            i32.const 1054629
            i32.const 2
            local.get 2
            local.get 0
            i32.add
            i32.const 128
            i32.add
            i32.const 0
            local.get 0
            i32.sub
            call $_ZN4core3fmt9Formatter12pad_integral17he52ae3771fdc9ec7E
            local.set 0
            br 1 (;@3;)
          end
          i32.const 0
          local.set 0
          loop  ;; label = @4
            local.get 2
            local.get 0
            i32.add
            i32.const 127
            i32.add
            local.get 4
            i32.const 15
            i32.and
            local.tee 3
            i32.const 48
            i32.or
            local.get 3
            i32.const 55
            i32.add
            local.get 3
            i32.const 10
            i32.lt_u
            select
            i32.store8
            local.get 0
            i32.const -1
            i32.add
            local.set 0
            local.get 4
            i32.const 4
            i32.shr_u
            i32.const 15
            i32.and
            local.tee 4
            br_if 0 (;@4;)
          end
          local.get 0
          i32.const 128
          i32.add
          local.tee 4
          i32.const 129
          i32.ge_u
          br_if 2 (;@1;)
          local.get 1
          i32.const 1
          i32.const 1054629
          i32.const 2
          local.get 2
          local.get 0
          i32.add
          i32.const 128
          i32.add
          i32.const 0
          local.get 0
          i32.sub
          call $_ZN4core3fmt9Formatter12pad_integral17he52ae3771fdc9ec7E
          local.set 0
        end
        local.get 2
        i32.const 128
        i32.add
        global.set 0
        local.get 0
        return
      end
      local.get 4
      i32.const 128
      call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
      unreachable
    end
    local.get 4
    i32.const 128
    call $_ZN4core5slice22slice_index_order_fail17hdb5bb7f5aa9f866cE
    unreachable)
  (func $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17hdc1b3aa87d1578c1E (type 2) (param i32 i32) (result i32)
    (local i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    block  ;; label = @1
      block  ;; label = @2
        local.get 0
        i32.load
        local.tee 0
        i32.load8_u
        i32.const 1
        i32.eq
        br_if 0 (;@2;)
        local.get 1
        i32.load offset=24
        i32.const 1057176
        i32.const 4
        local.get 1
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        call_indirect (type 6)
        local.set 1
        br 1 (;@1;)
      end
      local.get 2
      local.get 1
      i32.load offset=24
      i32.const 1057172
      i32.const 4
      local.get 1
      i32.const 28
      i32.add
      i32.load
      i32.load offset=12
      call_indirect (type 6)
      i32.store8 offset=8
      local.get 2
      local.get 1
      i32.store
      local.get 2
      i32.const 0
      i32.store8 offset=9
      local.get 2
      i32.const 0
      i32.store offset=4
      local.get 2
      local.get 0
      i32.const 1
      i32.add
      i32.store offset=12
      local.get 2
      local.get 2
      i32.const 12
      i32.add
      i32.const 1054612
      call $_ZN4core3fmt8builders10DebugTuple5field17h95b19566bf4f9168E
      drop
      local.get 2
      i32.load8_u offset=8
      local.set 1
      block  ;; label = @2
        local.get 2
        i32.load offset=4
        local.tee 3
        i32.eqz
        br_if 0 (;@2;)
        local.get 1
        i32.const 255
        i32.and
        local.set 0
        i32.const 1
        local.set 1
        block  ;; label = @3
          local.get 0
          br_if 0 (;@3;)
          block  ;; label = @4
            local.get 3
            i32.const 1
            i32.ne
            br_if 0 (;@4;)
            local.get 2
            i32.load8_u offset=9
            i32.const 255
            i32.and
            i32.eqz
            br_if 0 (;@4;)
            local.get 2
            i32.load
            local.tee 0
            i32.load8_u
            i32.const 4
            i32.and
            br_if 0 (;@4;)
            i32.const 1
            local.set 1
            local.get 0
            i32.load offset=24
            i32.const 1054608
            i32.const 1
            local.get 0
            i32.const 28
            i32.add
            i32.load
            i32.load offset=12
            call_indirect (type 6)
            br_if 1 (;@3;)
          end
          local.get 2
          i32.load
          local.tee 1
          i32.load offset=24
          i32.const 1054609
          i32.const 1
          local.get 1
          i32.const 28
          i32.add
          i32.load
          i32.load offset=12
          call_indirect (type 6)
          local.set 1
        end
        local.get 2
        local.get 1
        i32.store8 offset=8
      end
      local.get 1
      i32.const 255
      i32.and
      i32.const 0
      i32.ne
      local.set 1
    end
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    local.get 1)
  (func $_ZN57_$LT$core..str..Utf8Error$u20$as$u20$core..fmt..Debug$GT$3fmt17h3049a0f39aa8ac27E (type 2) (param i32 i32) (result i32)
    (local i32 i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee 2
    global.set 0
    local.get 1
    i32.load offset=24
    i32.const 1057185
    i32.const 9
    local.get 1
    i32.const 28
    i32.add
    i32.load
    i32.load offset=12
    call_indirect (type 6)
    local.set 3
    local.get 2
    i32.const 0
    i32.store8 offset=5
    local.get 2
    local.get 3
    i32.store8 offset=4
    local.get 2
    local.get 1
    i32.store
    local.get 2
    local.get 0
    i32.store offset=12
    local.get 2
    i32.const 1057194
    i32.const 11
    local.get 2
    i32.const 12
    i32.add
    i32.const 1057156
    call $_ZN4core3fmt8builders11DebugStruct5field17hdb0382cc3deab674E
    drop
    local.get 2
    local.get 0
    i32.const 4
    i32.add
    i32.store offset=12
    local.get 2
    i32.const 1057205
    i32.const 9
    local.get 2
    i32.const 12
    i32.add
    i32.const 1057216
    call $_ZN4core3fmt8builders11DebugStruct5field17hdb0382cc3deab674E
    drop
    local.get 2
    i32.load8_u offset=4
    local.set 1
    block  ;; label = @1
      local.get 2
      i32.load8_u offset=5
      i32.eqz
      br_if 0 (;@1;)
      local.get 1
      i32.const 255
      i32.and
      local.set 0
      i32.const 1
      local.set 1
      block  ;; label = @2
        local.get 0
        br_if 0 (;@2;)
        local.get 2
        i32.load
        local.tee 1
        i32.const 28
        i32.add
        i32.load
        i32.load offset=12
        local.set 0
        local.get 1
        i32.load offset=24
        local.set 3
        block  ;; label = @3
          local.get 1
          i32.load8_u
          i32.const 4
          i32.and
          br_if 0 (;@3;)
          local.get 3
          i32.const 1054603
          i32.const 2
          local.get 0
          call_indirect (type 6)
          local.set 1
          br 1 (;@2;)
        end
        local.get 3
        i32.const 1054602
        i32.const 1
        local.get 0
        call_indirect (type 6)
        local.set 1
      end
      local.get 2
      local.get 1
      i32.store8 offset=4
    end
    local.get 2
    i32.const 16
    i32.add
    global.set 0
    local.get 1
    i32.const 255
    i32.and
    i32.const 0
    i32.ne)
  (func $_ZN4core7unicode12unicode_data15grapheme_extend6lookup17he32874b852959152E (type 14) (param i32) (result i32)
    (local i32 i32)
    local.get 0
    i32.const 10
    i32.shr_u
    local.set 1
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          block  ;; label = @4
            local.get 0
            i32.const 125952
            i32.lt_u
            br_if 0 (;@4;)
            i32.const 30
            local.set 2
            local.get 1
            i32.const 896
            i32.eq
            br_if 1 (;@3;)
            i32.const 0
            return
          end
          local.get 1
          i32.const 1057232
          i32.add
          i32.load8_u
          local.tee 2
          i32.const 30
          i32.gt_u
          br_if 1 (;@2;)
        end
        local.get 2
        i32.const 4
        i32.shl
        local.get 0
        i32.const 6
        i32.shr_u
        i32.const 15
        i32.and
        i32.or
        i32.const 1057355
        i32.add
        i32.load8_u
        local.tee 1
        i32.const 138
        i32.gt_u
        br_if 1 (;@1;)
        local.get 1
        i32.const 3
        i32.shl
        i32.const 1057856
        i32.add
        i64.load
        i64.const 1
        local.get 0
        i32.const 63
        i32.and
        i64.extend_i32_u
        i64.shl
        i64.and
        i64.const 0
        i64.ne
        return
      end
      i32.const 1057124
      local.get 2
      i32.const 31
      call $_ZN4core9panicking18panic_bounds_check17h8c4f03235e0b5d9bE
      unreachable
    end
    i32.const 1057140
    local.get 1
    i32.const 139
    call $_ZN4core9panicking18panic_bounds_check17h8c4f03235e0b5d9bE
    unreachable)
  (table (;0;) 95 95 funcref)
  (memory (;0;) 17)
  (global (;0;) (mut i32) (i32.const 1048576))
  (global (;1;) i32 (i32.const 1059576))
  (global (;2;) i32 (i32.const 1059576))
  (export "memory" (memory 0))
  (export "init" (func $init))
  (export "process" (func $process))
  (export "shutdown" (func $shutdown))
  (export "allocate_buffer" (func $allocate_buffer))
  (export "drop_buffer" (func $drop_buffer))
  (export "__data_end" (global 1))
  (export "__heap_base" (global 2))
  (elem (;0;) (i32.const 1) $_ZN4core3ptr13drop_in_place17h5c9b794b2099b1eaE $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$8take_box17hfc597a3aff2741abE $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$3get17h003b888122c564eaE $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17h3d1c1bb0748b11daE $_ZN59_$LT$core..fmt..Arguments$u20$as$u20$core..fmt..Display$GT$3fmt17hd7c46cc3e94bd933E $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h18a30a599fe658f8E $_ZN4core3ptr13drop_in_place17ha97f2c661f1fe906E $_ZN61_$LT$serde_json..error..Error$u20$as$u20$core..fmt..Debug$GT$3fmt17h7e3fb3b68632231bE $_ZN4core3ptr13drop_in_place17h79d74b610ac14d1fE $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$8take_box17h7d082bbcb9b85d03E $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$3get17h2d6887d8afeafa81E $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17h6a14ba090fa87b57E $_ZN4core3ptr13drop_in_place17hbf7e19099f11a74dE $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_str17h50c0b4ca9feb25fcE $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$10write_char17hf74f26a32dd33facE $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_fmt17h0da91fd8f8378a91E $_ZN44_$LT$$RF$T$u20$as$u20$core..fmt..Display$GT$3fmt17h06b8d40f9bff9871E $_ZN4core3fmt3num3imp52_$LT$impl$u20$core..fmt..Display$u20$for$u20$u32$GT$3fmt17h976c74654a4bcc54E $_ZN58_$LT$alloc..string..String$u20$as$u20$core..fmt..Debug$GT$3fmt17hb1335b498f38353fE $_ZN4core3ptr13drop_in_place17h93127de8e088b4c0E $_ZN53_$LT$core..fmt..Error$u20$as$u20$core..fmt..Debug$GT$3fmt17h510f284fdbf86330E $_ZN45_$LT$$RF$T$u20$as$u20$core..fmt..UpperHex$GT$3fmt17h3969f366df3d1826E $_ZN60_$LT$std..io..error..Error$u20$as$u20$core..fmt..Display$GT$3fmt17hfbf3380c56bbc0baE $_ZN55_$LT$std..path..Display$u20$as$u20$core..fmt..Debug$GT$3fmt17h669142b7b6fe9e0fE $_ZN4core3fmt3num3imp52_$LT$impl$u20$core..fmt..Display$u20$for$u20$i32$GT$3fmt17hcfda14b3662191abE $_ZN60_$LT$alloc..string..String$u20$as$u20$core..fmt..Display$GT$3fmt17hf4bdb13e62be459bE $_ZN44_$LT$$RF$T$u20$as$u20$core..fmt..Display$GT$3fmt17h0488b928e863645bE $_ZN3std5alloc24default_alloc_error_hook17h38966f062fa7b248E $_ZN44_$LT$$RF$T$u20$as$u20$core..fmt..Display$GT$3fmt17hb3c88bf73f5dda2eE $_ZN91_$LT$std..sys_common..backtrace.._print..DisplayBacktrace$u20$as$u20$core..fmt..Display$GT$3fmt17hefefb5408add7eddE $_ZN4core3ptr13drop_in_place17h025d405e835bf726E $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_str17hcb1cbfc4948dd346E $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$10write_char17h3aa90f5249f239f0E $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_fmt17h9f3c0397574cf561E $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_str17h1bea73a017e579a3E $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$10write_char17h8a06baf7e7cbda40E $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_fmt17h62784e4259f969c4E $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h595c62cfe8d58551E $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17hcde11046253965a8E $_ZN63_$LT$core..cell..BorrowMutError$u20$as$u20$core..fmt..Debug$GT$3fmt17h77bcc250a44c6ae5E $_ZN4core3ptr13drop_in_place17h242a50c8a365baa2E $_ZN62_$LT$std..ffi..c_str..NulError$u20$as$u20$core..fmt..Debug$GT$3fmt17ha722d92d960e680dE $_ZN60_$LT$core..cell..BorrowError$u20$as$u20$core..fmt..Debug$GT$3fmt17hddc186737bf29b56E $_ZN57_$LT$core..str..Utf8Error$u20$as$u20$core..fmt..Debug$GT$3fmt17h3049a0f39aa8ac27E $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h316e40b6c6984df5E $_ZN4core3ptr13drop_in_place17h2a6c181863033dd6E $_ZN3std5error5Error5cause17h955b446571fa3458E $_ZN3std5error5Error7type_id17h64ce692d4319812fE $_ZN3std5error5Error9backtrace17h799bc3f420176d06E $_ZN243_$LT$std..error..$LT$impl$u20$core..convert..From$LT$alloc..string..String$GT$$u20$for$u20$alloc..boxed..Box$LT$dyn$u20$std..error..Error$u2b$core..marker..Sync$u2b$core..marker..Send$GT$$GT$..from..StringError$u20$as$u20$std..error..Error$GT$11description17hbaa389d79ee7d499E $_ZN244_$LT$std..error..$LT$impl$u20$core..convert..From$LT$alloc..string..String$GT$$u20$for$u20$alloc..boxed..Box$LT$dyn$u20$std..error..Error$u2b$core..marker..Sync$u2b$core..marker..Send$GT$$GT$..from..StringError$u20$as$u20$core..fmt..Display$GT$3fmt17h9ecb4409df561c37E $_ZN242_$LT$std..error..$LT$impl$u20$core..convert..From$LT$alloc..string..String$GT$$u20$for$u20$alloc..boxed..Box$LT$dyn$u20$std..error..Error$u2b$core..marker..Sync$u2b$core..marker..Send$GT$$GT$..from..StringError$u20$as$u20$core..fmt..Debug$GT$3fmt17h672b3228d831bc08E $_ZN4core3ptr13drop_in_place17hcd0ca0b4ad624647E $_ZN80_$LT$std..io..Write..write_fmt..Adaptor$LT$T$GT$$u20$as$u20$core..fmt..Write$GT$9write_str17h577dfb1621f2139fE $_ZN4core3fmt5Write10write_char17h9e8d63491b3d7364E $_ZN4core3fmt5Write9write_fmt17hbd9d6bdcf58a73f8E $_ZN4core3ptr13drop_in_place17hf93883f47f141821E $_ZN3std10sys_common9backtrace10_print_fmt28_$u7b$$u7b$closure$u7d$$u7d$17hb8b7b5b7c7952fd5E $_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h56b65778f93a50acE $_ZN60_$LT$std..io..stdio..StderrRaw$u20$as$u20$std..io..Write$GT$5write17hd1489a4d8b578e60E $_ZN3std2io5Write14write_vectored17h08df67e913c2a8b3E $_ZN59_$LT$std..process..ChildStdin$u20$as$u20$std..io..Write$GT$5flush17h9f125fd478950081E $_ZN3std2io5Write9write_all17ha85cbf0a744d542bE $_ZN3std2io5Write9write_fmt17h5ee903061f640461E $_ZN4core3ptr13drop_in_place17h08a8a1ed893b2be6E $_ZN3std2io5impls71_$LT$impl$u20$std..io..Write$u20$for$u20$alloc..boxed..Box$LT$W$GT$$GT$5write17h3f92adeca71419b2E $_ZN3std2io5impls71_$LT$impl$u20$std..io..Write$u20$for$u20$alloc..boxed..Box$LT$W$GT$$GT$14write_vectored17hfc1c93a246624d48E $_ZN3std2io5impls71_$LT$impl$u20$std..io..Write$u20$for$u20$alloc..boxed..Box$LT$W$GT$$GT$5flush17ha92b9fce67924e8eE $_ZN3std2io5impls71_$LT$impl$u20$std..io..Write$u20$for$u20$alloc..boxed..Box$LT$W$GT$$GT$9write_all17h58e4711e602e748fE $_ZN3std2io5impls71_$LT$impl$u20$std..io..Write$u20$for$u20$alloc..boxed..Box$LT$W$GT$$GT$9write_fmt17hed5dbf8ed70707e1E $_ZN4core3ptr13drop_in_place17he00a11dd07d03937E $_ZN90_$LT$std..panicking..begin_panic_handler..PanicPayload$u20$as$u20$core..panic..BoxMeUp$GT$8take_box17hf451b6db153646f3E $_ZN90_$LT$std..panicking..begin_panic_handler..PanicPayload$u20$as$u20$core..panic..BoxMeUp$GT$3get17h0cf460ea902fc052E $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17h310bf071aa6f797cE $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$8take_box17hb93df9b37cb795f9E $_ZN91_$LT$std..panicking..begin_panic..PanicPayload$LT$A$GT$$u20$as$u20$core..panic..BoxMeUp$GT$3get17h5a8f37f29fff8575E $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17h6f2f3a966973ed98E $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h61c13854410d74b4E $_ZN44_$LT$$RF$T$u20$as$u20$core..fmt..Display$GT$3fmt17h51bafaa45edf02f5E $_ZN71_$LT$core..ops..range..Range$LT$Idx$GT$$u20$as$u20$core..fmt..Debug$GT$3fmt17h9736df68fe193a38E $_ZN41_$LT$char$u20$as$u20$core..fmt..Debug$GT$3fmt17he8ac9a372b840f2eE $_ZN4core3fmt10ArgumentV110show_usize17hc56a657681711f3eE $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h343baa976fb42c89E $_ZN4core3ptr13drop_in_place17h01d2cdf0847f87a8E $_ZN36_$LT$T$u20$as$u20$core..any..Any$GT$7type_id17hb5955542a46d6244E $_ZN68_$LT$core..fmt..builders..PadAdapter$u20$as$u20$core..fmt..Write$GT$9write_str17hc704ee84ceffabf7E $_ZN4core3fmt5Write10write_char17hbc1fcddc054f7d7fE $_ZN4core3fmt5Write9write_fmt17h0a230916a217709bE $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17hb0114246c3496e11E $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_str17h0d9b9fc37108a31fE $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$10write_char17h5e8659a6ef4b958fE $_ZN50_$LT$$RF$mut$u20$W$u20$as$u20$core..fmt..Write$GT$9write_fmt17h3342b5868002fad5E $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17h8401558e7b8ffa27E $_ZN42_$LT$$RF$T$u20$as$u20$core..fmt..Debug$GT$3fmt17hdc1b3aa87d1578c1E)
  (data (;0;) (i32.const 1048576) "At the discotests/data/wasm/panic/src/lib.rs\0c\00\10\00 \00\00\00\0e\00\00\00\05\00\00\00\01\00\00\00\08\00\00\00\04\00\00\00\02\00\00\00\03\00\00\00\01\00\00\00\08\00\00\00\04\00\00\00\04\00\00\00Tried to shrink to a larger capacity<::core::macros::panic macros>\00\00\84\00\10\00\1e\00\00\00\02\00\00\00\02\00\00\00assertion failed: `(left == right)`\0a  left: ``,\0a right: ``: \b4\00\10\00-\00\00\00\e1\00\10\00\0c\00\00\00\ed\00\10\00\03\00\00\00destination and source slices have different lengths\08\01\10\004\00\00\00/rustc/b8cedc00407a4c56a3bda1ed605c6fc166655447/src/libcore/macros/mod.rs\00\00\00D\01\10\00I\00\00\00\12\00\00\00\0d\00\00\00internal error: entered unreachable code<::std::macros::panic macros>\00\00\00\c8\01\10\00\1d\00\00\00\02\00\00\00\04\00\00\00\22,\5ct\5cr\5cn\5cf\5cb\5c\5c\5c\22:}{rolecalled `Result::unwrap()` on an `Err` value\00\00\07\00\00\00\04\00\00\00\04\00\00\00\08\00\00\00lib/vector-wasm/src/hostcall.rs\00L\02\10\00\1f\00\00\00\07\00\00\00\12\00\00\00SinkSourceTransform\00\09\00\00\00\08\00\00\00\04\00\00\00\0a\00\00\00\0b\00\00\00\09\00\00\00\08\00\00\00\04\00\00\00\0c\00\00\00\0d\00\00\00\04\00\00\00\04\00\00\00\0e\00\00\00\0f\00\00\00\10\00\00\000123456789abcdefuuuuuuuubtnufruuuuuuuuuuuuuuuuuu\00\00\22\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\5c\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00Tried to shrink to a larger capacity<::core::macros::panic macros>\00\00\00\04\10\00\1e\00\00\00\02\00\00\00\02\00\00\000\04\10\00\00\00\00\00a Display implementation returned an error unexpectedly/rustc/b8cedc00407a4c56a3bda1ed605c6fc166655447/src/liballoc/string.rs\00\00\00o\04\10\00F\00\00\00|\08\00\00\09\00\00\00\14\00\00\00\00\00\00\00\01\00\00\00\15\00\00\00recursion limit exceededunexpected end of hex escapetrailing characterstrailing commalone leading surrogate in hex escapekey must be a stringcontrol character (\5cu0000-\5cu001F) found while parsing a stringinvalid unicode code pointnumber out of rangeinvalid numberinvalid escapeexpected valueexpected identexpected `,` or `}`expected `,` or `]`expected `:`EOF while parsing a valueEOF while parsing a stringEOF while parsing an objectEOF while parsing a listError(, line: , column: )\00\00\00\a0\06\10\00\06\00\00\00\a6\06\10\00\08\00\00\00\ae\06\10\00\0a\00\00\00\b8\06\10\00\01\00\00\00\1f\00\00\00\04\00\00\00\04\00\00\00 \00\00\00!\00\00\00\22\00\00\00\1f\00\00\00\04\00\00\00\04\00\00\00#\00\00\00$\00\00\00%\00\00\00\1f\00\00\00\04\00\00\00\04\00\00\00&\00\00\00already borrowed/rustc/b8cedc00407a4c56a3bda1ed605c6fc166655447/src/libcore/cell.rs\00,\07\10\00C\00\00\00n\03\00\00\09\00\00\00already mutably borrowed,\07\10\00C\00\00\00\1e\03\00\00\09\00\00\00\1f\00\00\00\00\00\00\00\01\00\00\00'\00\00\00`: called `Option::unwrap()` on a `None` value\00\00\1f\00\00\00\00\00\00\00\01\00\00\00(\00\00\00)\00\00\00\10\00\00\00\04\00\00\00*\00\00\00\1f\00\00\00\00\00\00\00\01\00\00\00+\00\00\00called `Result::unwrap()` on an `Err` value\00\1f\00\00\00\08\00\00\00\04\00\00\00,\00\00\00<::core::macros::panic macros>\00\00T\08\10\00\1e\00\00\00\02\00\00\00\02\00\00\00Tried to shrink to a larger capacity\1f\00\00\00\04\00\00\00\04\00\00\00-\00\00\00src/libstd/thread/mod.rsfailed to generate unique thread ID: bitspace exhausted\00\b8\08\10\00\18\00\00\00*\04\00\00\11\00\00\00\b8\08\10\00\18\00\00\000\04\00\00\16\00\00\00thread name may not contain interior null bytes\00\b8\08\10\00\18\00\00\00s\04\00\00\1a\00\00\00RUST_BACKTRACE0src/libstd/env.rsfailed to get environment variable `\88\09\10\00$\00\00\00\b8\07\10\00\03\00\00\00w\09\10\00\11\00\00\00\fb\00\00\00\1d\00\00\00.\00\00\00\0c\00\00\00\04\00\00\00/\00\00\000\00\00\001\00\00\002\00\00\00/\00\00\003\00\00\004\00\00\00\22data provided contains a nul byteunexpected end of fileother os erroroperation interruptedwrite zerotimed outinvalid datainvalid input parameteroperation would blockentity already existsbroken pipeaddress not availableaddress in usenot connectedconnection abortedconnection resetconnection refusedpermission deniedentity not found\00\a8\07\10\00\00\00\00\00 (os error )\a8\07\10\00\00\00\00\00H\0b\10\00\0b\00\00\00S\0b\10\00\01\00\00\00failed to write whole buffer5\00\00\00\0c\00\00\00\04\00\00\006\00\00\007\00\00\008\00\00\00formatter error\009\00\00\00\10\00\00\00\04\00\00\00:\00\00\00;\00\00\00note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.\0a\c4\0b\10\00X\00\00\00full<unknown>\5cx\001\0c\10\00\02\00\00\00\00\00\00\00 \00\00\00\08\00\00\00\02\00\00\00\00\00\00\00\00\00\00\00\02\00\00\00\03\00\00\00fatal runtime error: \0a\00\00\5c\0c\10\00\15\00\00\00q\0c\10\00\01\00\00\00memory allocation of  bytes failed\00\00\84\0c\10\00\15\00\00\00\99\0c\10\00\0d\00\00\00src/libstd/panicking.rs\00\b8\0c\10\00\17\00\00\00\ba\00\00\00\14\00\00\00Box<Any><unnamed>\00\00\00\1f\00\00\00\00\00\00\00\01\00\00\00<\00\00\00=\00\00\00>\00\00\00?\00\00\00@\00\00\00\00\00\00\00A\00\00\00\08\00\00\00\04\00\00\00B\00\00\00C\00\00\00D\00\00\00E\00\00\00F\00\00\00\00\00\00\00thread '' panicked at '', \00\00<\0d\10\00\08\00\00\00D\0d\10\00\0f\00\00\00S\0d\10\00\03\00\00\00q\0c\10\00\01\00\00\00note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\0a\00\00x\0d\10\00N\00\00\00\b8\0c\10\00\17\00\00\00z\01\00\00\0f\00\00\00\b8\0c\10\00\17\00\00\00{\01\00\00\0f\00\00\00G\00\00\00\10\00\00\00\04\00\00\00H\00\00\00I\00\00\00.\00\00\00\0c\00\00\00\04\00\00\00J\00\00\00\1f\00\00\00\08\00\00\00\04\00\00\00K\00\00\00L\00\00\00\1f\00\00\00\08\00\00\00\04\00\00\00M\00\00\00thread panicked while processing panic. aborting.\0a\00\008\0e\10\002\00\00\00thread panicked while panicking. aborting.\0a\00t\0e\10\00+\00\00\00failed to initiate panic, error \a8\0e\10\00 \00\00\00NulError\1f\00\00\00\04\00\00\00\04\00\00\00N\00\00\00cannot recursively acquire mutexsrc/libstd/sys/wasi/../wasm/mutex.rs\08\0f\10\00$\00\00\00\15\00\00\00\09\00\00\00strerror_r failuresrc/libstd/sys/wasi/os.rs\00N\0f\10\00\19\00\00\00#\00\00\00\0d\00\00\00N\0f\10\00\19\00\00\00%\00\00\00\09\00\00\00rwlock locked for writing\00\00\00\88\0f\10\00\19\00\00\00operation not supported on wasm yetstack backtrace:\0a\00\19\12D;\02?,G\14=30\0a\1b\06FKE7\0fI\0e\17\03@\1d<+6\1fJ-\1c\01 %)!\08\0c\15\16\22.\108>\0b41\18/A\099\11#C2B:\05\04&('\0d*\1e5\07\1aH\13$L\ff\00\00Success\00Illegal byte sequence\00Domain error\00Result not representable\00Not a tty\00Permission denied\00Operation not permitted\00No such file or directory\00No such process\00File exists\00Value too large for data type\00No space left on device\00Out of memory\00Resource busy\00Interrupted system call\00Resource temporarily unavailable\00Invalid seek\00Cross-device link\00Read-only file system\00Directory not empty\00Connection reset by peer\00Operation timed out\00Connection refused\00Host is unreachable\00Address in use\00Broken pipe\00I/O error\00No such device or address\00No such device\00Not a directory\00Is a directory\00Text file busy\00Exec format error\00Invalid argument\00Argument list too long\00Symbolic link loop\00Filename too long\00Too many open files in system\00No file descriptors available\00Bad file descriptor\00No child process\00Bad address\00File too large\00Too many links\00No locks available\00Resource deadlock would occur\00State not recoverable\00Previous owner died\00Operation canceled\00Function not implemented\00No message of desired type\00Identifier removed\00Link has been severed\00Protocol error\00Bad message\00Not a socket\00Destination address required\00Message too large\00Protocol wrong type for socket\00Protocol not available\00Protocol not supported\00Not supported\00Address family not supported by protocol\00Address not available\00Network is down\00Network unreachable\00Connection reset by network\00Connection aborted\00No buffer space available\00Socket is connected\00Socket not connected\00Operation already in progress\00Operation in progress\00Stale file handle\00Quota exceeded\00Multihop attempted\00Capabilities insufficient\00No error information\00\00src/liballoc/raw_vec.rscapacity overflow\00\00V\16\10\00\17\00\00\00\ea\02\00\00\05\00\00\00`\00..\92\16\10\00\02\00\00\00BorrowErrorBorrowMutErrorcalled `Option::unwrap()` on a `None` value: \00\00\90\16\10\00\00\00\00\00\e0\16\10\00\02\00\00\00T\00\00\00\00\00\00\00\01\00\00\00U\00\00\00:\00\00\00\90\16\10\00\00\00\00\00\04\17\10\00\01\00\00\00\04\17\10\00\01\00\00\00index out of bounds: the len is  but the index is \00\00 \17\10\00 \00\00\00@\17\10\00\12\00\00\00T\00\00\00\0c\00\00\00\04\00\00\00V\00\00\00W\00\00\00X\00\00\00     {\0a,\0a,  { } }(\0a(,)\0a[T\00\00\00\04\00\00\00\04\00\00\00Y\00\00\00]0x00010203040506070809101112131415161718192021222324252627282930313233343536373839404142434445464748495051525354555657585960616263646566676869707172737475767778798081828384858687888990919293949596979899\00T\00\00\00\04\00\00\00\04\00\00\00Z\00\00\00[\00\00\00\5c\00\00\00src/libcore/fmt/mod.rs\00\00\88\18\10\00\16\00\00\00D\04\00\00\0d\00\00\00\88\18\10\00\16\00\00\00P\04\00\00$\00\00\00src/libcore/slice/mod.rsindex  out of range for slice of length \d8\18\10\00\06\00\00\00\de\18\10\00\22\00\00\00\c0\18\10\00\18\00\00\00r\0a\00\00\05\00\00\00slice index starts at  but ends at \00 \19\10\00\16\00\00\006\19\10\00\0d\00\00\00\c0\18\10\00\18\00\00\00x\0a\00\00\05\00\00\00assertion failed: broken.is_empty()src/libcore/str/lossy.rs\00\87\19\10\00\18\00\00\00\9d\00\00\00\11\00\00\00src/libcore/str/mod.rs\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\01\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\02\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\03\04\04\04\04\04\00\00\00\00\00\00\00\00\00\00\00[...]byte index  is out of bounds of `\cb\1a\10\00\0b\00\00\00\d6\1a\10\00\16\00\00\00\90\16\10\00\01\00\00\00\b0\19\10\00\16\00\00\00S\08\00\00\09\00\00\00begin <= end ( <= ) when slicing `\00\00\14\1b\10\00\0e\00\00\00\22\1b\10\00\04\00\00\00&\1b\10\00\10\00\00\00\90\16\10\00\01\00\00\00\b0\19\10\00\16\00\00\00W\08\00\00\05\00\00\00\b0\19\10\00\16\00\00\00h\08\00\00\0e\00\00\00 is not a char boundary; it is inside  (bytes ) of `\cb\1a\10\00\0b\00\00\00x\1b\10\00&\00\00\00\9e\1b\10\00\08\00\00\00\a6\1b\10\00\06\00\00\00\90\16\10\00\01\00\00\00\b0\19\10\00\16\00\00\00j\08\00\00\05\00\00\00src/libcore/unicode/printable.rs\e4\1b\10\00 \00\00\00\1a\00\00\00(\00\00\00\00\01\03\05\05\06\06\03\07\06\08\08\09\11\0a\1c\0b\19\0c\14\0d\12\0e\0d\0f\04\10\03\12\12\13\09\16\01\17\05\18\02\19\03\1a\07\1c\02\1d\01\1f\16 \03+\04,\02-\0b.\010\031\022\01\a7\02\a9\02\aa\04\ab\08\fa\02\fb\05\fd\04\fe\03\ff\09\adxy\8b\8d\a20WX\8b\8c\90\1c\1d\dd\0e\0fKL\fb\fc./?\5c]_\b5\e2\84\8d\8e\91\92\a9\b1\ba\bb\c5\c6\c9\ca\de\e4\e5\ff\00\04\11\12)147:;=IJ]\84\8e\92\a9\b1\b4\ba\bb\c6\ca\ce\cf\e4\e5\00\04\0d\0e\11\12)14:;EFIJ^de\84\91\9b\9d\c9\ce\cf\0d\11)EIWde\8d\91\a9\b4\ba\bb\c5\c9\df\e4\e5\f0\04\0d\11EIde\80\81\84\b2\bc\be\bf\d5\d7\f0\f1\83\85\8b\a4\a6\be\bf\c5\c7\ce\cf\da\dbH\98\bd\cd\c6\ce\cfINOWY^_\89\8e\8f\b1\b6\b7\bf\c1\c6\c7\d7\11\16\17[\5c\f6\f7\fe\ff\80\0dmq\de\df\0e\0f\1fno\1c\1d_}~\ae\af\bb\bc\fa\16\17\1e\1fFGNOXZ\5c^~\7f\b5\c5\d4\d5\dc\f0\f1\f5rs\8ftu\96\97/_&./\a7\af\b7\bf\c7\cf\d7\df\9a@\97\980\8f\1f\c0\c1\ce\ffNOZ[\07\08\0f\10'/\ee\efno7=?BE\90\91\fe\ffSgu\c8\c9\d0\d1\d8\d9\e7\fe\ff\00 _\22\82\df\04\82D\08\1b\04\06\11\81\ac\0e\80\ab5\1e\15\80\e0\03\19\08\01\04/\044\04\07\03\01\07\06\07\11\0aP\0f\12\07U\08\02\04\1c\0a\09\03\08\03\07\03\02\03\03\03\0c\04\05\03\0b\06\01\0e\15\05:\03\11\07\06\05\10\07W\07\02\07\15\0dP\04C\03-\03\01\04\11\06\0f\0c:\04\1d%_ m\04j%\80\c8\05\82\b0\03\1a\06\82\fd\03Y\07\15\0b\17\09\14\0c\14\0cj\06\0a\06\1a\06Y\07+\05F\0a,\04\0c\04\01\031\0b,\04\1a\06\0b\03\80\ac\06\0a\06\1fAL\04-\03t\08<\03\0f\03<\078\08+\05\82\ff\11\18\08/\11-\03 \10!\0f\80\8c\04\82\97\19\0b\15\88\94\05/\05;\07\02\0e\18\09\80\b00t\0c\80\d6\1a\0c\05\80\ff\05\80\b6\05$\0c\9b\c6\0a\d20\10\84\8d\037\09\81\5c\14\80\b8\08\80\c705\04\0a\068\08F\08\0c\06t\0b\1e\03Z\04Y\09\80\83\18\1c\0a\16\09H\08\80\8a\06\ab\a4\0c\17\041\a1\04\81\da&\07\0c\05\05\80\a5\11\81m\10x(*\06L\04\80\8d\04\80\be\03\1b\03\0f\0d\00\06\01\01\03\01\04\02\08\08\09\02\0a\05\0b\02\10\01\11\04\12\05\13\11\14\02\15\02\17\02\19\04\1c\05\1d\08$\01j\03k\02\bc\02\d1\02\d4\0c\d5\09\d6\02\d7\02\da\01\e0\05\e1\02\e8\02\ee \f0\04\f9\06\fa\02\0c';>NO\8f\9e\9e\9f\06\07\096=>V\f3\d0\d1\04\14\1867VW\bd5\ce\cf\e0\12\87\89\8e\9e\04\0d\0e\11\12)14:EFIJNOdeZ\5c\b6\b7\1b\1c\a8\a9\d8\d9\097\90\91\a8\07\0a;>fi\8f\92o_\ee\efZb\9a\9b'(U\9d\a0\a1\a3\a4\a7\a8\ad\ba\bc\c4\06\0b\0c\15\1d:?EQ\a6\a7\cc\cd\a0\07\19\1a\22%>?\c5\c6\04 #%&(38:HJLPSUVXZ\5c^`cefksx}\7f\8a\a4\aa\af\b0\c0\d0\0cr\a3\a4\cb\ccno^\22{\05\03\04-\03e\04\01/.\80\82\1d\031\0f\1c\04$\09\1e\05+\05D\04\0e*\80\aa\06$\04$\04(\084\0b\01\80\90\817\09\16\0a\08\80\989\03c\08\090\16\05!\03\1b\05\01@8\04K\05/\04\0a\07\09\07@ '\04\0c\096\03:\05\1a\07\04\0c\07PI73\0d3\07.\08\0a\81&\1f\80\81(\08*\80\86\17\09N\04\1e\0fC\0e\19\07\0a\06G\09'\09u\0b?A*\06;\05\0a\06Q\06\01\05\10\03\05\80\8b` H\08\0a\80\a6^\22E\0b\0a\06\0d\139\07\0a6,\04\10\80\c0<dS\0c\01\80\a0E\1bH\08S\1d9\81\07F\0a\1d\03GI7\03\0e\08\0a\069\07\0a\816\19\80\c72\0d\83\9bfu\0b\80\c4\8a\bc\84/\8f\d1\82G\a1\b9\829\07*\04\02`&\0aF\0a(\05\13\82\b0[eK\049\07\11@\04\1c\97\f8\08\82\f3\a5\0d\81\1f1\03\11\04\08\81\8c\89\04k\05\0d\03\09\07\10\93`\80\f6\0as\08n\17F\80\9a\14\0cW\09\19\80\87\81G\03\85B\0f\15\85P+\80\d5-\03\1a\04\02\81p:\05\01\85\00\80\d7)L\04\0a\04\02\83\11DL=\80\c2<\06\01\04U\05\1b4\02\81\0e,\04d\0cV\0a\0d\03]\03=9\1d\0d,\04\09\07\02\0e\06\80\9a\83\d6\0a\0d\03\0b\05t\0cY\07\0c\14\0c\048\08\0a\06(\08\1eRw\031\03\80\a6\0c\14\04\03\05\03\0d\06\85jsrc/libcore/unicode/mod.rs\00I!\10\00\1a\00\00\008\00\00\00\0f\00\00\00I!\10\00\1a\00\00\009\00\00\00\10\00\00\00T\00\00\00\04\00\00\00\04\00\00\00]\00\00\00SomeNoneErrorUtf8Errorvalid_up_toerror_len\00\00T\00\00\00\04\00\00\00\04\00\00\00^\00\00\00\04\0f\15\1b\19\03\12\17\11\00\00\0e\16\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\06\13\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\02\07\0a\00\08\0c\1d\1c\18\1a\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\05\01\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\10\00\00\00\00\0b\00\09\00\14\00\0d\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\0f\12\00\00\00\00\00\00\00\00\00\00\00\00\00\1f\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00IFf\1d\00\00\00\00\00\00\00\00\00\00\00\00\8a>\00\00\00\00\00\00\00\00\00\00\00\00\00KS\00\00\00\00\00\00\00\00\00\00\00\00g#B\00\00\00\00\00\00\00\00\00\00\00\00=\00\00\00\00\00#\00\00\00\00\00\00\00\00\00u\00\00-\00\00\00\00\00\00\00\00\00\00\00\00\82N<\00\00\00\00\00\00\00\00\00\00\00\00c\00\00\00%\00Z\00\00\00\00\00\00\00\816\00\00\03\00\00\00\00\00\00\00\00\00\00/\00\00\00\00\00\00\00\00\10\00\00\00\00\00\13\00\08\00\00\00\00\00\00\00\00\00\00\00\00\00C\00r\00\89\00\00\00\00\00\00\00\00\00\00\07\00\00\00}\05\18?\007\87\09@d\00\00!\00\00\00\00\00\00\00\00\00\00\00\00\00\0a\00\00A\00\00\00\00\00\00\00\00\00\00\00\00\0c\000\00\5c\00\00\00\19wq\00`G5D.\00\00t9\11e,Q^\7fP\00\00\0041\00\00\00S\00\00\00\00\00\00:\00\00\00\008\1a\00\88_+ki]O]\84\80*h\14;\00\17\00\00\00\00\00\00\00\00\00\00\00\00\00U\00\00W\00\00\00\83\00\00\00\00\00\00\00\00Y\00\00\00\00\00\00&n\1b\16\00\00\00\00\00mJ\1c\00\00\00\00\00\00\00\00\00\00$\00\00|\00R\00{\06\15\00\00\00\00H\00\00\00\00~(v'l)\00\22[\0ea\0dVpb\04\85 x\02\00\00z\1ey\01T\003\00\00\00\86sX\00MEo\0bj\00\002lL\00\00\89\8a\00\00\8a\8a\8a>\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\00\00\00\00\0d\00\00\00\00\00\00\00\1c\00\00\00\00\00\00\00@\00\00\00\00\00\00\00\b6\00\00\00\00\00\00\00\bf\00\00\00\00\00\00\00\f8\03\00\00\00\00\00\00\f0\07\00\00\00\00\00\00\ff\07\00\00\00\00\00\00\00\10\00\00\00\00\00\00\00\1e\00\00\00\00\00\00\008\00\00\00\00\00\00\00?\00\00\00\00\00\00\80\7f\00\00\00\00\00\00\00\80\00\00\00\00\00\00\c0\ff\01\00\00\00\00\00\80\ff\03\00\00\00\00\00\00\80\07\00\00\00\00\00\00\00\7f\00\00\00\00\00\01 \80\00\00\00\00\00\00\00\a3\00\00\00\00\00\00\fc\7f\03\00\00\00\00\00\00\00\06\00\00\00\00\00\00\ff\07\00\00\00\00\00\00\80\09\00\00\00\00\00\00\00\0e\00\00\00\00\80\00~\0e\00\00\00\00d \00 \00\00\00\00@\fe\0f \00\00\00\00\01\00\000\00\00\00\00\00\00\00@\00\00\00\00\5c\00\00@\00\00\00\00\00\00\00`\00\00\00\00\00\84\5c\80\00\00\00\00\00\00\00\c0\00\00\00\00\00\00\00\e0\00\00\00\00\00\00\00\00\01\00\00\00\00\00\f0\0c\01\00\00\00D0`\00\0c\00\00\00\c1=`\00\0c\00\00\00\1e \80\00\0c\00\00\00\1e \c0\00\0c\00\00\00\fe!\fe\00\0c\00\00\00\00\00\00\00 \00\00\00\00\00\00\00`\00\00\00D\08\00\00`\00\00\00\00\00\00\00\f0\00\00\00`\00\00\00\00\02\00\00\7f\ff\ff\f9\db\07\00\00\00\00\00\80\f8\07\00\00\00\00\00\e0\bc\0f\00\00\00\00\00\00 !\00\00\03\00\00\00<;\00\00\e7\0f\00\00\00<\00\00\00\00\c0\9f\9f=\00\00\00\00\c0\fb\ef>\00\00\00\00\00\00\c0?\00\00\00\00\00\00\00\f0\00\00\00\00\00\00\00\fc\00\00\10\00\00\f8\fe\ff\00\00\ff\ff\00\00\ff\ff\00\00\ff\ff\ff\ff\ff\ff\00\00\00\f8\ff\ff\00\00\01\00\00\00\00\00\c0\ff\01\00\00\00\ff\ff\ff\ff\01\00\00\00\00\00\00\00\03\00\00\00\00\00\00\80\03\00\00\00\00\00@\a3\03\00\00\00\00\00\00\00\08\00\00\00\0c\00\00\00\0c\00\04\00\00\00\00\f8\0f\00\00\00\00\00\00\00\18\00\00\00\1c\00\00\00\1c\00\00\00\00\c3\01\00\1e\00\00\00\00\00\00\00\1f\00\01\00\80\00\c0\1f\1f\00\07\00\00\00\80\ef\1f\00\ff\ff\ff\ff\ff\1f \00\869\02\00\00\00#\00\02\00\00\00\000@\00\00\00\00\00\00~f\00\00\00\fc\ff\ff\fcm\00\00\00\00\00\00\00\7f\00\00\00\00\00\00(\bf\00\00\00\00\00\00\f0\cf\00\00\00\00\03\00\00\a0\02\00\00\f7\ff\fd!\10\03\03\00\00\00\00\00x\06\00\00\00\00\00\80\ff\06\00\00\00\00\00\00\c0\07\00\00\00\00\00\00\f2\07\00\00\00\00\87\01\04\0e\06\00\00\00\00\00\00\10\08\10\00\00\00\00\00\10\07\00\00\00\00\00\00\14\0f\00\00\00\00\00\f0\17\00\00\00\00\00\00\f2\1f\df\e0\ff\fe\ff\ff\ff\1f\00\00\00\00\00\00\00 \00\00\00\00\00\f8\0f \07\00\00\00\00\00\c83\00\00\00\00\00\00\b0?\00\00\00\00\00\80\f7?\04\00\00\00\00\00\00@\1e \80\00\0c\00\00@\00\00\00\00\00\80\d3@\02\00\00\00\00\00\00P\03\00\00\00\00\00\00X\00\00\00\00\00\e0\fdf\fe\07\00\00\00\00\f8y\03\00\00\00\00\00\c0\7f\00\00\00\00\00\00\fe\7f\00\00\00\00\00\00\ff\7f\00\00\00\00\00\00\00\80\7f\00\00\00\00\00\00\800\00\00\00\ff\ff\03\80n\f0\00\00\00\00\00\87\02\00\00\00\00\00\00\90\00\00@\7f\e5\1f\f8\9f\00\00\00\00\00\00\f9\a5\00\00\00\00\00\00\f8\a7\00\00\00\00\00\80<\b0\00\00\00\00\00\00~\b4\00\00\00\00\00\00\7f\bf\00\00\fe\ff\ff\ff\ff\bf\11\00\00\00\00\00\00\c0\00\00\00\00\00\00\9d\c1\02\00\00\00\00\00\00\d0\00\00\00\00\a0\c3\07\f8\ff\ff\ff\ff\ff\ff\7f\f8\ff\ff\ff\ff\ff\ff\ff\fb\be!\00\00\0c\00\00\fc\00\00\00\00\00\00\00\ff\02\00\00\00\00\00\00\ff\00\00\02\00\00\00\ff\ff\00\00\f8\ff\fb\ff\ff\ff\00\00\00\00\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff\ff")
  (data (;1;) (i32.const 1058968) "\01\00\00\00\00\00\00\00\01\00\00\00\d8*\10\00")
  (data (;2;) (i32.const 1058984) "\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00"))
