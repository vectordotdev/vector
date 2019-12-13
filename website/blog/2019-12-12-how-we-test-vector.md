---
id: how-we-test-vector
title: "How we test Vector"
description: "A survey of techniques we've found useful"
author_id: "luke"
tags: []
---

When we set out to build Vector, we knew that reliability and performance were
two of our top priorities. We also knew that even the best of intentions would
not be enough to make certain those qualities were realized and reflected in our
users' production deployments. Since then, we've been continuously evolving and
expanding our approach to achieving that level of quality.

<!--truncate-->

There are a few factors that make ensuring robustness a particularly difficult
task for software like Vector:

1. It's relatively new and not as "battle tested" as more widely deployed
   software.
1. The vast majority of its functionality lives at the edges, interfacing with
   various external systems.
1. Instead of a monolithic application designed for a single task, it is
   a collection of components that can be assembled into a near-infinite number
   of configurations.

While there's no one perfect solution to overcome these difficulties, we've been
able to apply a wide variety of testing techniques across different levels of
the system to help us build confidence in its reliability:

* Example-based testing
   * Unit tests
   * Integration tests
* Generative testing
   * Property-based testing
   * Model-based testing
   * Fuzz testing
* Black-box testing
   * Performance tests
   * Correctness tests
   * Reliability tests

In this post we'll try to lay out how we're using each of these, the types of
situations where they fit best, and how we approach writing each style.

## Example-based testing

We'll start off with the types of tests you're likely most familiar with, the
humble unit and integration tests. These are the bread and butter of almost
every test suite, and for good reason.

We group them into "example-based" because they both follow the same general
pattern. As the developer, you come up with an example setup or input, you have
your code process that example, and then you assert that the outcome is what you
expected.

If you're already a master of these common types of tests, feel free to skip to
the later sections.

### Unit tests

There has been a lot of discussion about what exactly qualifies as a unit test,
but for us it just means the general idea of running some predefined example
inputs through a simple, isolated function (i.e. no network calls, etc) and
checking that it returns the correct answer.

Unit tests, wherever possible, are where we like to start when introducing a new
component. They're relatively simple to write, reliable, and quick to run.
Whenever there is a piece of "business logic" (often identifiable as a cluster
of conditionals), we'll try to wrap some unit tests around it as early in the
development process as possible.

In Vector, we've found them to be a particularly great fit for our transforms.
This makes sense, since transforms are effectively just isolated functions that
take events in and return events out. For the same reason, we tend to use unit
tests extensively around the encoder portions of our sinks.

For other types of components, so much of the functionality is focused on
communication with external systems that it can be difficult to isolate logic
enough for a unit test. As much as possible, we try to critically assess that
difficulty and use it to help find ways we can refactor the design to be more
modular and therefore amenable to unit testing. Unit tests are a good source of
design feedback, so we want to feel that pain and refactor if they're difficult
to write.

The first place that unit tests start to fall short is for the code that's
leftover after you've extracted and tested all the simple functions that you
reasonably can. What remains is the "imperative shell" of the component, and
this tends to be where integration tests come into the picture.

The second place they feel limited is around functions that may be perfectly
well isolated, but have such a vast number of possible inputs that it seems
impossible to write enough examples to get sufficient coverage. This is where
the category of generative tests can come in handy.

### Integration tests

As mentioned above, integration tests tend to get pulled out when you can no
longer find a good way to unit test a component. The idea of isolation goes out
the window and these tests often require a running and prepared database,
a sibling service available on a particular port, or other arbitrarily complex
environmental circumstances.

This necessary lack of isolation often makes integration tests some of the more
frustrating tests in your suite. They can be tedious to write, slow to run, and
often flaky if their environment isn't set up just so.

For that reason, we try to minimize the number of integration tests we need to
write by minimizing the number of code paths we can't test with another method.
Ideally, all of the interesting decision making in a component would be well
covered by unit tests or similar, and we can get away with a small handful of
integration tests that simply ensure data is flowing through those smaller
isolated pieces in the expected way.

For software like Vector, we still end up writing a good number of integration
tests based solely on the number of external systems we're interacting with. And
there's nothing wrong with that! We do what we can to make those tests resilient
to environment issues, less dependent on timing, etc, and they fulfill the
purpose of giving us confidence that really does communicate with these external
systems correctly.

## Generative testing

When we talked about unit tests, we mentioned that one area where they start to
break down is when testing functions with a very large space of possible inputs.
While you can do your best to think about that input space and come up with the
edge cases that need handled, the problem is that you'll likely be thinking
about the same cases when writing your unit tests as you are when writing the
implementation. And since you're only human, there's no guarantee that those are
the only cases you'll need to think about.

These shortcomings of the human mind are what generative tests seek to address.
Instead of requiring the developer to come up with interesting inputs to feed
into the system, this category of test offloads some of that creativity (or busy
work, depending on your perspective) to the machine.

Since a computer will never get tired of generating new inputs, these types of
tests are often run differently than your finite number unit and integration
test. Sometimes they are included in the normal test suite but only run for
a given number of iterations, but other times they are treated as a separate
suite that gets run for extended periods of time or even indefinitely.

### Property-based tests

A simple way to think about property-based tests is that they are like unit
tests, but the computer comes up with the test inputs instead of the developer.
This immediately creates the problem that you don't necessarily know the correct
output for any given random input, since that's the whole point of the function
under test in the first place. In the absence of an oracle providing correct
answers, property-based tests focus on certain _properties_ that must hold for
any given input and output. The classic example is testing a function that
reverses a list by ensuring that any list reversed twice is equal to itself.

The canonical tool for property-based testing is [quickcheck][1], written in
Haskell around 20 years ago. Since then, implementations have popped up for most
popular languages. The python library [hypothesis][2] is another prominent tool,
with some different design decisions from classic quickcheck.

In addition to coming up with all kinds of example inputs for your tests, most
of these tools also have an important feature called shrinking. While the
details are complex, the basic idea is that the tool will do extra work to
simplify any failing examples before presenting them to you. This makes them
much easier to understand and therefore much more useful than the long,
convoluted input that may have initially triggered the failure.

In Vector, we use property-based tests to exercise our internal data
serialization logic. Because we store data in a flattened representation (for
now, at least), we want to make sure that arbitrary input events can go through
that flattening and unflattening process without losing information along the
way. We use the Rust [proptest][3] library to generate input events for us and
then give it the property that the output must equal the input after a round
trip of serialization and deserialization.

This type of test is a great candidate for many cases you would normally unit
test. Tests that are deterministic, quick to run, and free of environmental side
effects will generally produce the best results. You could apply the same
principles to other kinds of tests, but the process may be slower, less
reproducible, and with inputs that are harder to shrink.

### Model-based testing

One particularly interesting use of property-based testing tools is something
we've seen called [model-based testing][4]. In addition to your actual
system-under-test, you implement a dramatically simpler model of your system's
behavior. If you are writing a key-value database, for example, your model could
be a simple in-memory hashmap. You then you use a tool like quickcheck to
generate arbitrary sequences of operations to apply to both your system and the
model. The enforced property is that your system and your model produce the same
outputs.

Vector inherited one of these tests from [cernan][5], where its file tailing
module originated (huge thanks to [Brian Troutwine][6]). It works by generating
random sequences of file writes, reads, rotations, truncations, etc and applying
them to both a simple in-memory model of a filesystem as well as the actual file
system being tailed by our file watcher implementation. It then verifies that
the lines returned by our watcher are the same as those returned from the
simplified simulation.

In this strategy, the model is acting as an oracle and the quality of the test
depends on that oracle actually behaving correctly. That makes it a good match
for components with a relatively simple API but deeper inner complexity for
performance optimizations, persistence, etc.

### Fuzz testing

Closely related to property-based testing is the idea of fuzz testing. Fuzz
testing is, at its most simplistic, just feeding your program random data and
seeing if it breaks. There is a huge variety of tools and techniques for
accomplishing this, many of which are remarkably sophisticated.

One of the most popular and influential fuzz testing tools is [american fuzzy
lop][7]. It is a coverage-guided, genetic fuzzer, which means that it watches
your program's execution with various random inputs and uses that information to
evolve new inputs that cover as many execution paths as possible. This makes it
extremely effective at find edge cases where your program could crash, hang,
etc.

Fuzzing is an extremely powerful technique for testing parsers. While it's easy
for something like quickcheck to generate random strings, it's not so easy to
come up with random strings that explore each and every branch of something like
a parser, at least in any reasonable amount of time. The feedback loop that
a fuzzer has access to allows it to zero in on interesting inputs far more
quickly than something simply generating random data.

Many of the parsers we use in Vector are prebuilt for various data formats and
have seen some fuzz testing in their upstream library. We did, however, write
our `tokenizer` parser from scratch and it's unique in that's it's not for
a specific format. Instead, it gives a best-effort attempt at breaking the input
up into logical fields. We've found it to be a great fit for fuzz testing
because the way that it handles strange and misshappen inputs is less important
than that fact that it will not panic and crash the program.

Because of the way that a fuzzer instruments your program to track code coverage
and observe crashes, it's generally not something used in the same way as unit
tests you run on every build. There is a bit of setup required to expose
a target that directly accepts raw bytes, build it with the proper
instrumentation, and wrap it to be driven by the fuzzer. Since we are not
actively changing the tokenizer parser, we simply did a session of fuzzing
manually on a developer's machine (using the excellent [`cargo-fuzz`][13]),
collected failing inputs and addresses them one by one, continuing until the
fuzzer had run for a significant amount of time without finding any crashes.
Finally, we wrote [unit tests][14] around those inputs to catch any regressions.
As we do more active development of our own parsers in the future, we will
likely invest in automation to run these fuzz tests continuously against our
master branch.

One of the limitations of AFL-style fuzzing is the focus on random byte strings
as inputs. This matches up really well with parsers, but maybe not that many
other components in your system. The idea of [structure-aware][12] fuzzers looks
to address this. One such tool is [fuzzcheck][8], which we've been starting to
explore. Instead of byte strings, it works directly with the actual types of
your program. It also runs in-process with your system, making it simpler to
detect not just panics but also things like simple test failures. In many ways,
it has the potential to combine the best of both fuzz testing and property-based
testing.

## Black-box testing

Even if all of the above testing strategies worked flawlessly and got us to 100%
branch coverage, we still wouldn't know that Vector was performing at the level
we expect. To answer that question, we need to run it as users run it and
observe things like throughput, memory usage, CPU usage, etc.

This is where the [`vector-test-harness`][9] tests come in. These are
high-level, black-box tests where we run various Vector configurations on
deployed hardware, generating load and capturing metrics about its performance.
And since they're black-box tests, we can also provide configurations for
similar tools to see how they compare.

### Performance tests

The performance tests in our harness focus on generating as much load as the
given configuration can handle and measuring throughput, memory use, etc. These
tests capture our real-world performance in way that microbenchmarks can't, and
they give us a very useful point of comparison with other tools that may have
made different design decisions. If one of the metrics looks way off, that gives
us a starting point to investigate why we're not performing as well as we think
we should.

Since these tests are almost completely automated, we'll soon be looking to
start running them on a nightly basis and graphing the results over time. This
should give us an early warning signal in the case of a serious performance
regression, and help us visualize our progress in making Vector faster and more
efficient over time.

### Correctness tests

Alongside those performance tests, we also have a set of tests we call
correctness tests. The setup is quite similar to the performance tests, but the
focus is different. Instead of generating as much load as we can and watching
things like throughput and system resource use, we instead run each
configuration through different interesting scenarios to see how they behave.

For example, we have correctness tests around various flavors of file rotation,
disk persistence across restarts, nested JSON messages, etc. While these are
behaviors that we also test at various lower levels (e.g. unit and integration
tests), covering a handful of important cases at this level of abstraction gives
us some extra confidence that we are seeing exactly what our users will see.

The ability to compare behaviors across competing tools is another bonus. Going
through the process of setting up those tests gets us valuable experience
working with those other tools. We can see what works well in their
configuration, documentation, etc, and identify areas where we can improve
Vector as a result.

### Reliability tests

A third category that we're currently working to integrate into
`vector-test-harness` is something we're calling reliability tests. These are
similar to the performance and correctness tests, except that they're designed
to run continuously and flush out errors that may occur only in rare
environmental circumstances.

In a way, they're like integration-level fuzz tests where changes in the
environment over time provide input randomness. For example, running a week-long
reliability test of our S3 sink exposed a bug where a specific kind of network
failure could lead to duplicate data when the retried request crossed
a timestamp boundary. That is not the type of failure we expect to be able to
induce in a local integration test, and the relevant factors (time and network
conditions) were not those exercised by standard fuzzing or property-based
testing.

The main challenge with these kinds of tests, aside from getting the requisite
environment and harnesses up and running, is capturing sufficient context about
the environment at the time of the failure that you stand a chance at
understanding and reproducing it. This task itself is a great test for our
observability, and any issue we can't reproduce is a sign that our logging and
metrics data needs to be improved.

Another issue with these tests is that the vast majority of the time, nothing
particularly interesting is happening. Since we want to find bugs as quickly as
possible, we can supplement the randomness of the environment by injecting
various types of faults on our own. There are a variety of tools for this, such
as [Toxiproxy][10] and [Namazu][11].

## Conclusion

Even with all of the above in place, we're continuously exploring ways to
further increase our confidence in the reliability and performance of Vector.
That could mean anything from expanding our current test suites to be more
thorough to adopting entirely new techniques to help cover more possible
executions (e.g. [simulation][15] or [metamorphic][16] testing).

When users are often running a Vector process on nearly every host in their
infrastructure, ensuring an extremely high level of robustness and efficiency
are paramount. At the same time, those needs must be balanced with increasing
Vector's functional capabilities. Finding the right balance will be an ongoing
challenge as the project grows and matures.



[1]:http://www.cse.chalmers.se/~rjmh/QuickCheck/manual.html
[2]: https://hypothesis.works/articles/what-is-property-based-testing/
[3]: https://github.com/AltSysrq/proptest
[4]: https://medium.com/@tylerneely/reliable-systems-series-model-based-property-testing-e89a433b360
[5]: https://github.com/postmates/cernan
[6]: https://github.com/blt
[7]: http://lcamtuf.coredump.cx/afl/
[8]: https://github.com/loiclec/fuzzcheck-rs
[9]: https://github.com/timberio/vector-test-harness/
[10]: https://github.com/Shopify/toxiproxy
[11]: https://github.com/osrg/namazu
[12]: https://github.com/google/fuzzing/blob/master/docs/structure-aware-fuzzing.md
[13]: https://github.com/rust-fuzz/cargo-fuzz
[14]: https://github.com/timberio/vector/blob/9fe1eeb4786b27843673c05ff012f6b5cf5c3e45/src/transforms/tokenizer.rs#L240-L249
[15]: https://www.youtube.com/watch?v=4fFDFbi3toc
[16]: https://www.hillelwayne.com/post/metamorphic-testing/



