---
title: Vector Performance Engineering
short: vector-perf
description: TODO
authors: ["blt"]
date: TODO
tags: ["performance engineering"]
---

In this post I'll describe how the Vector project maintains and improves its
high throughput performance, discussing the difficulties of writing
high-performance software on modern computers in general and the techniques
we've used to create a performance-oriented development culture around the
project. I'll start off by describing the general state of the art, take a
little tour through the Vector codebase to point out where we're doing the
typical things you'll find in a high-performance Rust project and, finally,
describe what we're doing here that's novel.

## Consider Profilers

Writing high-performance software is tough these days. It always _has been_
tough, requiring careful attention to how software gets put together and details
of the machine it runs on but we make it uncommonly hard on ourselves these
days. Let's consider profilers as a tool for a second. There's a small handful
of ways you can build them. In the old days -- which includes these days -- a
profiler worked by periodically pausing its own execution, calculting a
backtrace and then summing those backtraces up. This implies that the profiler
and your program are integrated in some way.

Let's make this more explicit. A tidy profiler with this approach is
[pprof][pprof-rs]. From [their
project](https://github.com/tikv/pprof-rs/blob/3fed55af8fc6cf69dbd954a0321c799c5a111e4e/examples/prime_number.rs):

```rust
// Copyright 2019 TiKV Project Authors. Licensed under Apache-2.0.

use pprof;

#[inline(never)]
fn is_prime_number(v: usize, prime_numbers: &[usize]) -> bool {
    if v < 10000 {
        let r = prime_numbers.binary_search(&v);
        return r.is_ok();
    }

    for n in prime_numbers {
        if v % n == 0 {
            return false;
        }
    }

    true
}

#[inline(never)]
fn prepare_prime_numbers() -> Vec<usize> {
    // bootstrap: Generate a prime table of 0..10000
    let mut prime_number_table: [bool; 10000] = [true; 10000];
    prime_number_table[0] = false;
    prime_number_table[1] = false;
    for i in 2..10000 {
        if prime_number_table[i] {
            let mut v = i * 2;
            while v < 10000 {
                prime_number_table[v] = false;
                v += i;
            }
        }
    }
    let mut prime_numbers = vec![];
    for i in 2..10000 {
        if prime_number_table[i] {
            prime_numbers.push(i);
        }
    }
    prime_numbers
}

fn main() {
    let prime_numbers = prepare_prime_numbers();

    let guard = pprof::ProfilerGuard::new(100).unwrap();

    loop {
        let mut v = 0;

        for i in 2..50000 {
            if is_prime_number(i, &prime_numbers) {
                v += 1;
            }
        }

        println!("Prime numbers: {}", v);

        match guard.report().build() {
            Ok(report) => {
                println!("{:?}", report);
            }
            Err(_) => {}
        };
    }
}
```

This program pre-populates table of known prime numbers -- that's
`prepare_prime_numbers` -- and then loops through all integers from 2 to 50,000
tallying up how many of them are actually prime, with `is_prime_number` being
the function on the hot path here. That's the majority thing this program does,
but the secondary function is to measure itself, the hook being this:

```rust
let guard = pprof::ProfilerGuard::new(100).unwrap();
```

The magic number `100` is the "frequency" with which pprof pauses the majority
project and calculates a backtrace. This is done by using [setitimer][setitimer]
fundamentally. This syscall lets us -- 'we' here being people with acess to a
Posix-ish system like Linux -- set up an 'interval timer' of three flavors, one
of which is `ITIMER_PROF` that "counts down against the total (i.e., both user
and system) CPU time consumed by the process" and once it expires generates "a
`SIGPROF` signal". This signal is caught by pprof's signal handler -- setup via
[`sigaction`][sigaction] -- and in that handler a backtrace is calculated,
ultimately via [libunwind][libunwind] through [backtrace-rs][backtrace-rs]. If
you want to trace through exactly how this works the function you want is
[`perf_signal_handler`](https://github.com/tikv/pprof-rs/blob/3fed55af8fc6cf69dbd954a0321c799c5a111e4e/src/profiler.rs#L239)
but any further discussion is outside the scope of this post. Suffice it to say,
the basic algorithm is this:

* Firstly, combine your target to be profiled with the profiler code.
* Establish a periodic `SIGPROF`.
* In the `SIGPROF` handler calculate a backtrace since last `SIGPROF`.
* Sum all the backtraces at end-of-program and report to the user.

The sum of backtraces tells you which functions your program traversed most
often. In the program above `main` will be entered and exited once,
`prepare_prime_numbers` also in and out once and `is_prime_number` will be
traversed just shy of 50,000 times. This approach is straightforward to
implement and has the benefit of participating in a long history of Unix
profilers. It does have some downsides, namely:

* Periodic `SIGPROF` handling acts as an 'overhead' on the program being
  measured.
* Details about the callstack lets us _infer_ where the program spent its time
  while executing, but it's not a direct measure.
  * If the program was executing across multiple threads the inference here gets
    even weaker.
* This style of profiler is only suitable for optimizing the CPU resource.

I want to elaborate on that last point before going forward. When a program runs
on a computer it is competing for resources with other programs running on the
system, itself and with the limits of the machine its running on. Optimizing a
bit of software isn't just "making it faster" -- and, in what way, by improving
latency or throughput? -- but is rather a game where the resource(s) that factor
into the optimization aim of the program are determined, measured and
characterized. If, for example, you want to improve the memory use of a program
you might go around the project shave bytes out of structs, reducing the number
of allocations etc etc but such work won't show up in a stack sampling profiler
like pprof. To be clear, that project does call itself a "CPU profiler" but this
is merely an example. Even with regard to the CPU resource, this kind of
sampling approach doesn't get at signals that are important on modern computers.
optimization takes place in terms of some resource limitation. If the prime
table were implemented as a linked list this program would run quite a bit
slower on a modern computer. Why? Well, modern CPUs have a [cache][cpu-cache]
that allows instructions executing on the CPU to avoid slow trips out to main
memory only so long as the memory needed is mapped into the CPUs' caches. A Rust
`Vec<T>` lays out instances of `T` in contiguous memory -- mostly, the internal
details are more complicated than this -- and when our program accesses some
index `i` the CPU will pull in a "block" from the memory that makes up the
vector around `i`. Excellent if we're sequentially examining the memory of a
vec. A linked list lays each of its `T` values out in memory randomly, in
addition to overhead incurred in node size to accomidate the pointers necessary
to navigate the list. These details of cache are _invisible_ to sampling
profilers even while they have a significant impact on program runtime
behavior. (Consider that the above program _might_ be faster if we ditched the
binary search -- which bounces around through memory -- for a sequential scan
but that such a change will be largely invisible to the profiler, except where
the `binary_search` disappears from the backtrace.)

These downsides are improvable to a great degree but they require more active
participation from either the kernel or the CPU itself. The [`perf`][linux-perf]
tool that ships on Linux systems is one such example. It's a suite of tools that
records 'perf events' from the kernel, via [`perf_event_open`][perf-event-open]
syscall and its relatives. Perf events are hardware oriented -- how many branch
mispredictions, cache misses, CPU cycles consumed -- and software oriented --
how many page misses, context switches, CPU migrations etc. The signal surface
is substantially richer but

[pprof-rs]: https://github.com/tikv/pprof-rs
[setitimer]: https://man7.org/linux/man-pages/man2/setitimer.2.html
[sigaction]: https://man7.org/linux/man-pages/man2/sigaction.2.html
[libunwind]: https://github.com/libunwind/libunwind
[backtrace-rs]: https://github.com/rust-lang/backtrace-rs
[cpu-cache]: https://en.wikipedia.org/wiki/CPU_cache
[linux-perf]: https://perf.wiki.kernel.org/index.php/Main_Page
[perf-event-open]: https://man7.org/linux/man-pages/man2/perf_event_open.2.html
