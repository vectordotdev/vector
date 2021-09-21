---
title: "How we test Vector"
description: "A survey of techniques we've found useful"
date: "2020-07-13"
authors: ["lukesteensen"]
badges:
  type: post
tags: ["testing", "tests"]
---

When we set out to build Vector, we knew that reliability and performance were
two of our top priorities. We also knew that even the best of intentions would
not be enough to make certain those qualities were realized and reflected in our
users' production deployments. Since then, we've been continuously evolving and
expanding our approach to achieving that level of quality.

<!--more-->

There are a few factors that make ensuring robustness a particularly difficult
task for software like Vector:

1. It's relatively new and not as "battle tested" as more widely deployed
   software.
1. The vast majority of its functionality lives at the edges, interfacing with
   various external systems.
1. Instead of a monolithic application designed for a single task, it is
   a collection of components that can be assembled into a near-infinite number
   of configurations.

This challenge has given us a unique opportunity to apply wide variety of
testing techniques:

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

While there's no one perfect solution to overcome these difficulties, we've
found that the combination of these techniques at different layers of the stack
has given us a good level of confidence in Vector's behavior.

In this post, we'll discuss briefly how we're using each of these types of
tests, their strengths and weaknesses, as well as any tips we have for using
them effectively.

## Example-based testing

We'll start off with the types of tests you're likely most familiar with, the
humble unit and integration tests. These are the bread and butter of almost
every test suite, and for good reason.

We group them into "example-based" because they both follow the same general
pattern. As the developer, you come up with an example input, you have your code
process that example, and then you assert that the outcome is what you expected.

A great deal has been written on these types of tests already, so we'll try to
keep this section brief and focused on situations where these techniques start
to break down.

### Unit tests

A unit test is generally defined by the idea of isolation. This can mean
different things to different people, but a common definition is that the test
is both isolated from the outside world (i.e. no network calls, reading files,
etc) and exercises a single component of the system (e.g. one function, class,
etc).

In Vector, we've found unit tests to be a particularly great fit for our
transforms. This makes sense, since transforms are effectively isolated
functions that take events in and return events out. For the same reason, we
tend to use unit tests extensively around the encoder portions of our sinks.

For other types of components, so much of the functionality is focused on
communication with external systems that it can be difficult to isolate logic
enough for a unit test. As much as possible, we try to critically assess that
difficulty and use it to help find ways we can refactor the design to be more
modular and therefore amenable to unit testing. Unit tests are an excellent
source of design feedback, so we want to feel that pain and refactor when
they're difficult to write.

That being said, there are two specific places we often run into the limitations
of unit tests. The first and more obvious is around pieces of code that are
fundamentally not isolated. The second situation has to do with the size of the
input space and number of potential paths through the component under test. As
an example-based testing strategy, the effectiveness of unit tests comes down to
the developer's ability to provide a thorough set of example inputs. This
becomes exponentially more difficult with each logical branch, and requires
recognizing paths you weren't considering when you initially wrote the code.

Takeaways:

* Isolation makes unit tests simple, fast, and reliable.
* If something is difficult to unit test, refactor until it's easy.
* As a human, be wary of your ability to think up exhaustive example inputs.

### Integration tests

The category of integration tests is a bit of a catch-all. Roughly defined,
they're example-based tests that are explicitly not isolated and focus on the
interaction between two or more components.

Given Vector's focus on integrating with a wide variety of external systems, we
have a higher ratio of integration tests than your average system. Even once
we've done all we can to isolate logic into small, unit-testable functions, we
still need to ensure the component as a whole does what it's supposed to. If
unit tests tell you that something works in theory, integration tests tell you
that it _should_ work in practice.

Similar to unit tests, there are two major downsides to integration tests. The
first is the same problem unit tests have with exhaustiveness, but intensely
magnified. Because they often cover the interaction of multiple whole systems,
integration tests will virtually never cover the combinatorial explosion of
potential execution paths. Second, integration tests are often simply a pain to
write and maintain. Given their dependence on external systems, they can be
tedious to write, slow to run, and often flaky if their environment isn't set up
just so.

Takeaways:

* While they can be a pain, integration tests are a valuable sanity check that
  your system will actually work for users.
* Don't overuse them trying to cover every possible scenario, because you can't.

## Generative testing

The idea of generative testing is a direct response to the shared shortcoming of
example-based strategies like unit and integration testing. Humans are bad at
visualizing very large state spaces, and the state space of your code's possible
inputs and execution paths is enormous. You also bring all the same biases when
writing tests as you did when writing the original code.

Generative tests address these issues by taking the human out of the equation
and having the computer generate millions of example inputs. The trade-off is
that since you aren't simply hard-coding a list of inputs and their expected
outputs anymore, you're forced to come up with more creative ways of identifying
failures.

### Property-based tests

A simple way to think about property-based tests is that they are unit tests
where the computer comes up with random example inputs. Since the test author
can no longer provide the expected output along with those example inputs, they
instead declare certain _properties_ that must hold true across any combination
of input and output. A classic example is testing a function that reverses
a list against the property that any list reversed twice must be equal to
itself.

While this is something you could do with no tooling support at all, there are
some pretty advanced libraries that make things easier (e.g. the venerable
[quickcheck][1] and more recent [hypothesis][2]). Beyond simple random input
generation, these tools often include valuable features like customizable
generators and shrinking, which attempts to simplify failing inputs as much as
possible before presenting them to you.

In Vector, we use property-based tests to exercise our internal data
serialization logic. We want to make sure that arbitrary input events can go
through the full serialization and deserialization process without losing any
information along the way. We use the Rust [proptest][3] library to generate
input events and then test that the output event is equal to the input after
a full round trip through the system. Since this is an isolated, deterministic
function, we can quickly and easily run millions of iterations of this test.

While property-based tests are certainly better at covering the input space than
unit tests, simple random generation of possible inputs can become a limitation.
Without some sense of what makes an input "interesting", property-based tests
have no way of intelligently exploring the space. In some cases, this can
meaning burning a lot of CPU without necessarily finding any new failures.

Takeaways:

* Property-based tests help uncover more edge cases in your system's logic.
* Like unit tests, they're most effective when applied to isolated components.
* They can't directly test "correctness", only that your given set of invariants
  is upheld.

### Model-based testing

One particularly interesting application of property-based test tooling is
something we've seen called [model-based testing][4]. The basic idea is that you
implement a simplified model of your system (e.g. a hashmap is a simple model of
a key-value store), and then assert that for all possible inputs, your system
should produce the same output as the model.

Vector inherited some of these tests from [cernan][5], where its file tailing
module originated (thanks [Brian][6]!). It works by generating random sequences
of file writes, reads, rotations, truncations, etc, and applying them to both
a simple in-memory model of a file system as well as the actual file system being
tailed by our file watcher implementation. It then verifies that the lines
returned by our watcher are the same as those returned from the simplified
simulation.

In this strategy, the model is acting as an oracle and the quality of the test
depends on that oracle actually behaving correctly. That makes it a good match
for components with a relatively simple API but deeper inner complexity due to
performance optimizations, persistence, etc. Like normal property-based tests,
they may have trouble efficiently exploring the state space of especially
complex components.

Takeaways:

* Model-based tests are a good match for components with deep implementations
  but relatively shallow APIs.
* They rely on a model implementation simple enough to be "obviously correct",
  which is not possible for all systems.

### Fuzz testing

At its most simplistic, fuzz testing is just feeding your program random data
and seeing if it breaks. In that sense, you can think of it as a kind of
external property-based testing, where the property is that your system should
not crash. This might not sound terribly interesting on its own, but modern
tools (e.g. [american fuzzy lop][7]) have developed a superpower that gives them
a massive advantage over traditional property-based testing: using code-coverage
information to guide input generation.

With this critical feedback loop in place, tools can see when a particular input
led to a new execution path. They can then intelligently evolve these
interesting inputs to prioritize finding even more new paths, zeroing in far
more efficiently on potential edge cases.

This is a particularly powerful technique for testing parsers. Where normal
property-based tests might repeatedly attempt to parse random strings and never
happen upon anything remotely valid, a fuzz testing tool can gradually "learn"
the format being parsed and spend far more time exploring productive areas of
the input space.

Many of the parsers we use in Vector are pre-built for various data formats and
have seen some fuzz testing in their upstream library. We did, however, write
our `tokenizer` parser from scratch and it's unique in that it's not for
a specific format. Instead, it gives a best-effort attempt at breaking the input
up into logical fields. We've found it to be a great fit for fuzz testing
because the way that it handles strange and misshapen inputs is less important
than that fact that it will not panic and crash the program.

One of the limitations of AFL-style fuzzing is the focus on random byte strings
as inputs. This matches up really well with parsers, but maybe not that many
other components in your system. The idea of [structure-aware][12] fuzzers looks
to address this. One such tool is [fuzzcheck][8], which we've been starting to
explore. Instead of byte strings, it works directly with the actual types of
your program. It also runs in-process with your system, making it simpler to
detect not just panics but also things like simple test failures. In many ways,
it has the potential to combine the best of both fuzz testing and property-based
testing.

Takeaways:

* Feedback loops allow fuzz testing to efficiently explore extremely large input
  spaces, like those of a parser.
* Tools are advancing rapidly, making fuzz tests more convenient for more types
  of situations.

## Black-box testing

Even if all of the above testing strategies worked flawlessly and got us to 100%
branch coverage, we still wouldn't know for certain that Vector was performing
at the level we expect. To answer that question, we need to run it as users run
it and observe things like throughput, memory usage, CPU usage, etc.

This is where the [`vector-test-harness`][9] comes in. These are high-level,
black-box tests where we run various Vector configurations on deployed hardware,
generating load and capturing metrics about its performance. And since they're
black-box tests (i.e. they require no access to or knowledge of Vector
internals), we can also provide configurations for similar tools to see how they
compare.

### Performance tests

The performance tests in our harness focus on generating as much load as the
given configuration can handle and measuring throughput, memory use, etc. These
tests capture our real-world performance in way that micro-benchmarks can't, and
they give us a very useful point of comparison with other tools that may have
made different design decisions. If one of the metrics looks way off, that gives
us a starting point to investigate why we're not performing as well as we think
we should.

Since these tests are almost completely automated, we'll soon be looking to
start running them on a nightly basis and graphing the results over time. This
should give us an early warning signal in the case of a serious performance
regression, and help us visualize our progress in making Vector faster and more
efficient over time.

Takeaways:

* Behavior under load is an important part of the user experience and deserves
  a significant testing investment.
* Regular, automated testing can generate valuable data for catching performance
  issues before they reach users.

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

Takeaways:

* Taking time to "zoom out" and test your system as a user would can help
  uncover blind spots and sanity-check behavior.
* Evaluating similar tools can help build a better understanding of user
  expectations.

### Reliability tests

A third category that we're currently working to integrate into
`vector-test-harness` is something we're calling reliability tests. These are
similar to performance and correctness tests, except that they're designed to
run continuously and flush out errors that may occur only in rare environmental
circumstances.

In a way, they're like simple, integration-level fuzz tests where changes in the
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
internal observability, and any issue we can't reproduce is a sign that our
logging and metrics data needs to be improved.

Another issue with these tests is that the vast majority of the time, nothing
particularly interesting is happening. Since we want to find bugs as quickly as
possible, we can supplement the randomness of the environment by injecting
various types of faults on our own. There are a variety of tools for this, such
as [Toxiproxy][10] and [Namazu][11].

Takeaways:

* The environment is an important source of uncertainty in your system that is
  difficult to simulate accurately.
* Observing bugs from a user's perspective incentivizes good internal
  observability tooling

## Conclusion

Even with all of the above in place, we're continuously exploring ways to
further increase our confidence in the reliability and performance of Vector.
That could mean anything from expanding our current test suites to be more
thorough to adopting entirely new techniques to help cover more possible
executions (e.g. [simulation][15] or [metamorphic][16] testing).

With some users running a Vector process on nearly every host in their
infrastructure, ensuring an extremely high level of robustness and efficiency is
paramount. At the same time, those needs must be balanced with increasing
Vector's functional capabilities. Finding the right balance is an ongoing
challenge as the project grows and matures.



[1]:http://www.cse.chalmers.se/~rjmh/QuickCheck/manual.html
[2]: https://hypothesis.works/articles/what-is-property-based-testing/
[3]: https://github.com/AltSysrq/proptest
[4]: https://medium.com/@tylerneely/reliable-systems-series-model-based-property-testing-e89a433b360
[5]: https://github.com/postmates/cernan
[6]: https://github.com/blt
[7]: http://lcamtuf.coredump.cx/afl/
[8]: https://github.com/loiclec/fuzzcheck-rs
[9]: https://github.com/vectordotdev/vector-test-harness/
[10]: https://github.com/Shopify/toxiproxy
[11]: https://github.com/osrg/namazu
[12]: https://github.com/google/fuzzing/blob/master/docs/structure-aware-fuzzing.md
[13]: https://github.com/rust-fuzz/cargo-fuzz
[14]: https://github.com/vectordotdev/vector/blob/9fe1eeb4786b27843673c05ff012f6b5cf5c3e45/src/transforms/tokenizer.rs#L240-L249
[15]: https://www.youtube.com/watch?v=4fFDFbi3toc
[16]: https://www.hillelwayne.com/post/metamorphic-testing/
