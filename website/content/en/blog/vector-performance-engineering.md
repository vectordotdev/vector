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
is substantially richer and the overhead lower, although not nil. To get some
sense of what working with `perf` is like I'm going to rewrite the above program
slightly:

```rust
// This code adapated from
// https://github.com/tikv/pprof-rs/blob/3fed55af8fc6cf69dbd954a0321c799c5a111e4e/examples/prime_number.rs. Licensed
// under Apache-2.0 by TiKV project authors, adaption by Brian L. Troutwine.

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
    let mut prime_number_table: [bool; 10_000] = [true; 10_000];
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
    for (i, is_prime) in prime_number_table.iter().enumerate().skip(2) {
        if *is_prime {
            prime_numbers.push(i);
        }
    }
    prime_numbers
}

fn main() {
    let prime_numbers = prepare_prime_numbers();

    let total_primes = (2..10_000_000).fold(0, |mut acc, i| {
        if is_prime_number(i, &prime_numbers) {
            acc += 1;
        }
        acc
    });

    println!("Prime numbers: {}", total_primes);
}
```

Of note here I've removed the `pprof` related material as `perf` is an external
profiler and does not need to be compiled into the program. We have the option
of 'attaching' `perf` to long-running processes but here we'll rely on `perf` to
run our program as a sub-process. The `Cargo.toml` for this program has some
important details:

```toml
[package]
name = "is_prime"
version = "0.1.0"
edition = "2021"

[dependencies]

[profile.release]
opt-level = 3
lto = true
codegen-units = 1

[profile.release-debug]
inherits = "release"
debug = true
```

No dependencies here, but the `release` build fiddles with some optimization
settings -- details [here][cargo-book-profiles] -- and we make a custom profile
`release-debug` that is exactly our release build but with debugging symbols. In
particular our binary will have [DWARF][dwarf] symbols embedded and we'll need
to instruct `perf` to use DWARF when constructing the call-graph of our
program. This is a consequence of the Linux kernel team not wanting to embed
DWARF decoding into the kernel itself, discussion of which is outside the scope
of this post. Anyhow, let's run compile this program and run it under `perf`:

```
> cargo build --profile release-debug
   Compiling is_prime v0.1.0 (/home/blt/projects/us/troutwine/is_prime)
    Finished release-debug [optimized + debuginfo] target(s) in 3.27s
> perf record --call-graph=dwarf --event=cpu-cycles,branches,branch-misses,cache-references,cache-misses ./target/release-debug/is_prime
Prime numbers: 664579
[ perf record: Woken up 663 times to write data ]
[ perf record: Captured and wrote 165.828 MB perf.data (20581 samples) ]
```

As mentioned, we inform `perf` that it should use DWARF when building the
call-graph -- if you forget this the program will appear very sparse -- when
recording, and collect CPU cycle, branch, branch misprediction, cache reference
and cache reference miss information. Exactly what information is available will
vary by host, use `perf list` to get the details for your system. You should now
see a `perf.data` file in your current working directory, in my case it's
165.828 MB large. Given the brief run of the program in question, `perf`
collected _a lot_ of data in a hurry.

There are a number of ways to interact with the data collected here, but before
we start aimlessly doing so let's put in mind what we want to understand about
this program, or what we want to optimize? Do we want it to run faster, use less
memory, run the same speed or better but have less cache misses? Having a
_specific goal_ will inform our investigation and let us guesstimate which
signals are valuable in measuring progress toward that goal. So, since we've
already measured cpu-cycles let's set as our goal reducing CPU cycles in
subsequent runs. If you run `perf report` in your current working directory
you'll drop into a TUI and one of the filter events will be cpu-cycles; navigate
to that and hit enter. In my most recent run I see 6696713386 cycles were
spent. You can advance to a symbol and hit + to open up the call-graph. Here's
what I see when I expand `is_prime::is_prime_number` some:

```
# Samples: 6K of event 'cpu-cycles:u'
# Event count (approx.): 6696713386
#
# Children      Self  Command   Shared Object         Symbol
# ........  ........  ........  ....................  ...................................................................................................
#
    99.99%     0.00%  is_prime  is_prime              [.] _start
            |
            ---_start
               __libc_start_main
               __libc_init_first
               main
               std::sys_common::backtrace::__rust_begin_short_backtrace
               core::ops::function::FnOnce::call_once (inlined)
               is_prime::main
               |
                --99.80%--core::iter::traits::iterator::Iterator::fold (inlined)
                          is_prime::main::{{closure}} (inlined)
                          |
                           --99.57%--is_prime::is_prime_number
                                     |
                                     |--21.59%--<usize as core::ops::arith::Rem<&usize>>::rem (inlined)
                                     |          <usize as core::ops::arith::Rem>::rem (inlined)
                                     |
                                      --10.94%--<core::slice::iter::Iter<T> as core::iter::traits::iterator::Iterator>::next (inlined)
```

Okay, sure. As expected `is_prime_number` represents the majority CPU cycles in
this program but, ah, we're spending 21% of our cycles running [`rem`][rem]? I
suppose that does make sense. The existing prime table is only 10,000 wide, so
the loop binary search in the prime number table only happens 1% of the time,
meaning that `v % n == 0` would be traversed quite often. At this point there
are two paths available to the would-be optimizer:

1. Can we make a faster `%`?
1. Can we make a program that gets to the same result but does less work?

The first question implies a sort of brute force approach, leave the basics of
the program in place but find less CPU cycle intensive alternatives for bits and
pieces. The second implies a kind of side-step of the problem by discovering a
new algorithm for the problem at hand, in which case we ask ourselves what
_really_ needs to be computed. To illustrate that second notion, consider that
`prepare_prime_numbers` allocates a new `Vec<usize>` with a known constant
size. Do we _really_ need to do that? In fact, no. But, it's worth understanding
that even if we shrink from `usize` to `bool` our search in the primes table
will be much larger because the number of primes in the range under discussion
is less than the total of integers, meaning we'll incur more `%` but have,
possibly, less cache thrash. At least in my run of the program,
`is_prime_number` participates in 89% of cache misses, primarily through
iteration. Or maybe we should increase the prime table size? Ah, or, considering
that primality tests are well-studied we might profitably spend our time hitting
the background literature; surely there's a method out there that improves over
this one, although it might be more complicated to understand and implement.

All this is to illustrate that while `perf` can give us a good deal of
information about what our programs are doing -- perhaps even an overwhelming
amount -- it can be hard to know what to do with it and there's often no "right"
answer. Profilers can tell us what a program did but can't tell us, considering
our optimization goals, what we should do _to_ our programs to improve them: we
are left to infer, with a combination of empirical data and expert knowledge of
the program and its machine where changes can be profitably made. In such a
small program as the prime tester, sure, expert knowledge is feasible but the
larger the program and the more it interacts with systems you have no control
over the harder it is to have that kind of insight, to the point where 'expert'
understanding converts to 'reasonably informed' in large projects.

## Statistical Games with Machines

Even if we assume perfect knowledge of the machine our programs run on there's
another problem that's implicit here that needs to be brought to the forefront:
the machine is non-deterministic and our programs are too. The consequence of
this might not be terribly apparent to you, and me too. In fact, the impact of
tihs is such that it was worth a paper in 2013, Curtsinger and Berger's
["STABILIZER: Statistically Sound Performance Evaluation"][stabilizer]. Before
you open that paper and dig through, think for a second how you might have
tested optimizations done on the primality program above. What I commonly see
folks do is a variation on "run `time` in a small loop", something like:

```
> time ./target/release-debug/is_prime
Prime numbers: 664579
9.93user 0.00system 0:09.93elapsed 99%CPU (0avgtext+0avgdata 1976maxresident)k
0inputs+0outputs (0major+86minor)pagefaults 0swaps
> time ./target/release-debug/is_prime
Prime numbers: 664579
9.90user 0.00system 0:09.90elapsed 99%CPU (0avgtext+0avgdata 1732maxresident)k
0inputs+0outputs (0major+84minor)pagefaults 0swaps
> time ./target/release-debug/is_prime
Prime numbers: 664579
9.92user 0.00system 0:09.92elapsed 99%CPU (0avgtext+0avgdata 1940maxresident)k
0inputs+0outputs (0major+86minor)pagefaults 0swaps
> time ./target/release-debug/is_prime
Prime numbers: 664579
9.91user 0.00system 0:09.92elapsed 99%CPU (0avgtext+0avgdata 1968maxresident)k
0inputs+0outputs (0major+86minor)pagefaults 0swaps
```

The number of memory pages vary slightly between runs, as do page faults but,
eh, on the whole the program, we can guess, runs in about 9 seconds, at least on
the machine I'm using. What if we `strip` the program of its debug symbols,
using the release profile build and running a `strip` pass just to be on the
safe side?

```
> time ./target/release/is_prime
Prime numbers: 664579
8.02user 0.00system 0:09.02elapsed 99%CPU (0avgtext+0avgdata 1876maxresident)k
0inputs+0outputs (0major+85minor)pagefaults 0swaps
> time ./target/release/is_prime
Prime numbers: 664579
8.87user 0.00system 0:08.89elapsed 99%CPU (0avgtext+0avgdata 1796maxresident)k
0inputs+0outputs (0major+85minor)pagefaults 0swaps
> time ./target/release/is_prime
Prime numbers: 664579
8.88user 0.00system 0:08.89elapsed 99%CPU (0avgtext+0avgdata 1860maxresident)k
0inputs+0outputs (0major+87minor)pagefaults 0swaps
> time ./target/release/is_prime
Prime numbers: 664579
8.96user 0.00system 0:08.96elapsed 99%CPU (0avgtext+0avgdata 1620maxresident)k
0inputs+0outputs (0major+82minor)pagefaults 0swaps
```

Interesting. We haven't changed the source of the program, just the layout of
the compiled binary and it's _faster_. Not significantly, mind, but
perceptibly. Or, is it? Let's run the program again:

```
> time ./target/release/is_prime
Prime numbers: 664579
9.33user 0.00system 0:09.34elapsed 99%CPU (0avgtext+0avgdata 1804maxresident)k
0inputs+0outputs (0major+87minor)pagefaults 0swaps
> time ./target/release/is_prime
Prime numbers: 664579
9.54user 0.00system 0:09.54elapsed 99%CPU (0avgtext+0avgdata 1656maxresident)k
0inputs+0outputs (0major+83minor)pagefaults 0swaps
> time ./target/release/is_prime
Prime numbers: 664579
9.83user 0.00system 0:09.83elapsed 99%CPU (0avgtext+0avgdata 1796maxresident)k
0inputs+0outputs (0major+85minor)pagefaults 0swaps
> time ./target/release/is_prime
Prime numbers: 664579
9.59user 0.00system 0:09.61elapsed 99%CPU (0avgtext+0avgdata 1796maxresident)k
0inputs+0outputs (0major+86minor)pagefaults 0swaps
```

I happen to be writing at present on a Thinkpad and I'm running these
experiments on the same device. What can account for these differences and can
we control for them? Well, one approach is to use a statistical approach to
measuring the behavior of a program. On a modern computer these and other
factors might influence how quickly a program runs, independent of the program
being run:

* branch misprediction
* cache behavior
* CPU frequency scaling
* the location of the program in memory
* scheduling by the kernel onto CPU cores
* etc

We can see above, for instance, that although the _same_ program is run it is
given slightly different totals of memory pages by the kernel, which leads to
slightly different page fault totals. In a short run this doesn't matter
overmuch, but that will add up over time. What can be done? Well, first, we need
tools that automatically re-run our programs a "statistically significant"
number of times to understand what it's overall behavior is in the space of its
probable behaviors. One of my favorites is [hyperfine][hyperfine].

TODO demonstrate hyperfine, show it off in the context of is_prime and note that one problem is it doesn't work for long-running programs

The STABILIZER paper argues that this is not totally sufficient:

> Unfortunately, even when using current best practices (large numbers of runs
> and a quiescent system), this approach is unsound. The problem is due to the
> interaction between software and modern architectural features, especially
> caches and branch predictors. These features are sensitive to the addresses of
> the objects they manage. Because of the significant performance penalties
> imposed by cache misses or branch mispredictions (e.g, due to aliasing), their
> reliance on addresses makes software exquisitely sensitive to memory
> layout. Small changes to code, adding or removing a stack variable, or
> changing the order of heap allocations can have a ripple effect that alters
> the placement in memory of every other function, stack frame, and heap object.
>
> The effect of these changes is unpredictable and substantial: [Mytkowicz et
> al.][producing-wrong-data] show that just changing the size of environment
> variables can trigger performance degradation as high as 300%; we find that
> simply changing the link order of object files can cause performance to
> decrease by up to 57%.

The paper then goes on to describe a very clever, invasive system that
automatically explores the memory layout space but does require you to specially
compile your program to get it. Happily on a modern Linux system we _sort of_
get some of the benefit through [ASLR](https://lwn.net/Articles/569635/)



There are many, many factors that influence runtimes.


method  _That_ question is what motivates the STABILIZER paper. The
authors note that "

## Micro-Benchmarks



It might be worthwhile

[pprof-rs]: https://github.com/tikv/pprof-rs
[setitimer]: https://man7.org/linux/man-pages/man2/setitimer.2.html
[sigaction]: https://man7.org/linux/man-pages/man2/sigaction.2.html
[libunwind]: https://github.com/libunwind/libunwind
[backtrace-rs]: https://github.com/rust-lang/backtrace-rs
[cpu-cache]: https://en.wikipedia.org/wiki/CPU_cache
[linux-perf]: https://perf.wiki.kernel.org/index.php/Main_Page
[perf-event-open]: https://man7.org/linux/man-pages/man2/perf_event_open.2.html
[cargo-book-profiles]: https://doc.rust-lang.org/cargo/reference/profiles.html
[dwarf]: https://dwarfstd.org/
[rem]: https://doc.rust-lang.org/std/ops/trait.Rem.html
[stabilizer]: https://people.cs.umass.edu/~emery/pubs/stabilizer-asplos13-draft.pdf
[producing-wrong-data]: https://users.cs.northwestern.edu/~robby/courses/322-2013-spring/mytkowicz-wrong-data.pdf
[aslr]: https://lwn.net/Articles/569635/
[hyperfine]: https://github.com/sharkdp/hyperfine

- - -

DRAGONS

Piece _might_ get up to chapter size following the A Week on the Concord and
Merrimack Rivers model. Ben's thoughts:

    Audience: Bottom up - Engineers, SREs, etc. The people actually using Vector. Hackernews types.
    Takeaway: “Vector is the fastest. Making Vector fast requires way more engineering than I thought. Brian Troutwine is smart. I’ll just use Vector.” :sunglasses:
    Goal: To establish Vector’s place in the market as the fastest.
    Structure (less concerned about this, do what you think is best as long as it accomplishes the above):

    Back story / problem - Who are you? Why are you writing this post? What can the reader expect to gain from reading?
    Journey to becoming the fastest data router:

    First and foremost - accurate signal and regression control (soak framework)
    High impact changes that the soaks drove: batching, etc, etc,

    Before / after performance results

With this audience we could short cut some of the detail here, assume knowledge of:

* profiler basics, skip to perf and how its signal doesn't lead to action but inference
* measurement difficulty RE statisticall stability, long-running daemons
* describe in detail regression detector, limitations
* probably most interesting to do release over release comparisons, bisect interesting changes between
  * issue with backward incompatibility, Luke has scraped data
