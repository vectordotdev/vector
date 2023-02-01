# RFC 3480 - 2020-10-15 - File Source Rework

The `file` source is one of Vector's oldest and most widely used components.
While it began with relatively simple interface and implementation, it has
collected a large number of knobs, warts, and general issues over time. To
ensure that this important source continues to serve users well, we should
invest effort into making it easier to use, extend, and debug.

## Scope

This RFC will cover a general overhaul of the `file` source internals and
user-facing config. The focus will be on migrating the existing implementation
to a better structure, not rewriting from scratch.

## Motivation

There are three pages of issues tagged `source: file` in the Vector repo. Below
I've tried to collect some of the more notable ones and categorize them:

* File source retains files on disk ([#763](https://github.com/vectordotdev/vector/issues/763))

  * Slowness due to many files
  * Need sequential instead of fair reads
  * Unclear why files handles were open
  * Unclear that files were not done being processed

* Checksums are confusing ([#828](https://github.com/vectordotdev/vector/issues/828))

  * Checksums don't work with small files

* `start_at_beginning` is confusing ([#1020](https://github.com/vectordotdev/vector/issues/1020))

  * Config behavior does not match user expectations

* Only warn about small files not empties ([#1065](https://github.com/vectordotdev/vector/issues/1065))

  * Checksums don't work with small files
  * Poor observability

* Split reads ([#1125](https://github.com/vectordotdev/vector/issues/1125) and [#2992](https://github.com/vectordotdev/vector/issues/2992))

  * Correctness

* Periodically read a non-file file ([#1198](https://github.com/vectordotdev/vector/issues/1198))

  * New mode

* Clean up file checkpoint files ([#1427](https://github.com/vectordotdev/vector/issues/1427))

  * Checkpointing

* File source slows down with many completed files ([#1466](https://github.com/vectordotdev/vector/issues/1466))

  * Slowness due to many files
  * Read scheduling

* Add path-based fingerprinting ([#1948](https://github.com/vectordotdev/vector/issues/1948))

  * Checksums don't work with small files
  * inode fingerprinting is fraught

* Log permissions issues ([#2420](https://github.com/vectordotdev/vector/issues/2420))

  * Poor observability

* Stop after reading files (backfill mode) ([#3216](https://github.com/vectordotdev/vector/issues/3216))

  * New mode

* Tailing limited to single core ([#3379](https://github.com/vectordotdev/vector/issues/3379))

  * Performance

* Not releasing descriptors due to falling behind ([#3440](https://github.com/vectordotdev/vector/issues/3440))

  * Need fair reads instead of sequential reads
  * Unclear why files handles were open
  * Unclear that files were not done being processed

* `ignore_older` is confusing ([#3567](https://github.com/vectordotdev/vector/issues/3567))

  * Config behavior does not match user expectations

* Ignore dangling symlinks (optionally?) ([#3662](https://github.com/vectordotdev/vector/issues/3662))

  * Tweak for specific use case

* Make it easier to reuse internally ([#4048](https://github.com/vectordotdev/vector/issues/4048))

* Batch mode ([#4271](https://github.com/vectordotdev/vector/issues/4271))

  * New mode

* Tail only new data ([#4382](https://github.com/vectordotdev/vector/issues/4382))

  * Config behavior does not match user expectations

* Slow with millions of files on EBS ([#4434](https://github.com/vectordotdev/vector/issues/4434))

  * Slowness due to many files
  * Need sequential instead of fair reads
  * New mode

The goal of this RFC is not to fix all of these issues in one fell swoop, but to
suggest a reorganization that renders the relevant behaviors as orthogonal as
possible and enables addressing them easily and independently.

## Internal Proposal

In order to achieve our goals, we need to break down the mostly-monolithic
`file` source into smaller components that can be understood, configured, and
extended in isolation, and then assembled smoothly into a cohesive unit.

To start, let's examine the current top-level structure of the `file` source.
This doesn't cover every interesting thing we want to address, but it's useful
context. In very rough pseudocode, it does the following:

```rust
let checkpoints = load_checkpoints();

// Find files we're configured to watch
let file_list = look_for_files();

// Prioritize files that exist on startup
sort(file_list);

loop {
    // Do these things occasionally to avoid burning CPU
    if its_time() {
        checkpoints.persist();
        let current_file_list = look_for_files();
        reconcile(&mut file_list, current_file_list);
    }

    for file in file_list {
        // Don't check inactive files as often
        if !should_read(file) {
          continue;
        }

        // Try to read new data from the files
        while let Some(line) = file.read_line() {
          output.push(line)
          // But not an infinite amount at one time
          if limit_reached { break }
        }

        // If configured, rm files when we're done with them
        maybe_rm(file)

        // Either continue to read the next file, or break to start back at the
        // beginning of the (prioritized) list
        if should_not_read_next_file() {
          break
        }
    }

    // Drop handles of deleted files that we've finished reading
    unwatch_dead(&mut file_list);

    // Send the gathered data downstream
    emit(output);

    // If we're not seeing any new data, back off to avoid burning CPU
    maybe_backoff();

    // If Vector is shutting down, stop processing
    maybe_shutdown();
}
```

Excepting observability and performance, the primary user-facing concerns in the
issues above are roughly as follows:

* Read scheduling
* File identity
* File starting point

These concerns are a good guide for determining which parts of the
implementation tend to change together.  Our first goal is to isolate those
areas of change from one another and construct subcomponents that effectively
contain complexity. With those seams in place, it should become simpler to
improve the internals of each.

### Subcomponents

Based on the user-level concerns we collected from the list of issues, let's
break down what each respective subcomponent should look like.

#### Read scheduling

In the pseudocode above, read scheduling is controlled by `sort`, `should_read`,
`limit_reached`, `should_not_read_next_file`, and `maybe_backoff`. A component
would need to answer the following questions:

1. In what order should I read the available files?
1. Should I finish one file before moving on to the next?
1. Should I back off reads to this file?
1. Should I back off reads to all files (i.e. sleep)?
1. How long should I spend working on a single file?

These questions would then be answered by a combination of configuration, file
metadata, and gathered statistics.

There are (at least) two approaches we could take to building this component.
The most obvious would be to implement a struct that contains configuration and
exposes methods very much like the ones mentioned above. This would maintain the
current structure of one main read loop with multiple points of control. An
alternative would be to introduce a trait representing the logic of the read
loop. This may lead to some duplication, but would allow simpler separation of
different use cases and require readers of the code understand the interaction
between fewer subtle points of control.

I would propose that we begin with the simpler method of consolidating logic
from the existing structure. This involves fewer design decisions and we may
find with the other simplifications we're planning make that further simplifying
the read loop is not worthwhile. That being said, once related features (like
the batch mode discussed later) are complete, we can reevaluate this decision.

#### File identity

Identity is an area where we already have the beginning of modularity with the
`Fingerprinter` abstraction, but it's incomplete. In our pseudocode, identity is
involved in both `look_for_files` and `reconcile`. It's not just a way to figure
out a magic identifier, but also the logic to update our list of watched files
based on those identifiers. It needs to answer the following:

1. Given a visible path, does it contain a file I've seen before?
1. If I have seen it before, has it been renamed?
1. If I have seen it before, am I now seeing it in multiple places?
1. If I'm seeing duplicates, how should I choose which to follow?

This logic is mostly in one place in the current implementation, so it should
not be terribly difficult to extract it. The use of `Fingerprinter` should
likely become purely internal to the new component.

In addition to simply consolidating the logic, we can expand and make the use of
`Fingerprinter` more intelligent. We currently have three ways it can work:

1. Checksum (usually reliable but frustrating for small files)
1. Device and inode (simple and works with small files, but doesn't handle edge
   cases well)
1. First line checksum (solid for intended use case but not yet general)

I'd first propose that we drop device and inode fingerprinting and add
path-based fingerprinting in its place. This gives users the option to do the
simplest possible thing for use cases that don't need to worry about traditional
rotation.

Next, I suggest we unify the two checksumming strategies. Neither is perfectly
general, but we should be able to come up with a simple algorithm that combines
the best of both. As a prerequisite, we should unify the read path such that
these checksums handle compressed files correctly (discussed later). With that
in place, the algorithm can look something like the following:

1. Read up to `max_line_length` bytes from the file starting at
   `ignored_header_bytes`
1. Return no fingerprint if there is no newline in the returned bytes
1. Otherwise, return the checksum of the bytes up to the first newline

This should give a good balance between usability and flexibility for the
default strategy. As we implement it, we should evolve the current
representation of fingerprints to one that maintains information about how it
was determined. This will allow us more flexibility to evolve and/or combine
strategies in the future.

#### File starting point

This logic is one layer below what's represented in the pseudocode above, but
there is just as much complexity. When we `look_for_files` and build watchers
for them, we need to make a decision about where to start reading in that file.
That decision should be based on any stored checkpoint, file metadata (e.g.
mtime), whether the file was found at startup or runtime, and how the source is
configured.

Since this decision really only happens in one place, the challenge is more
about providing an understandable config UI than designing the right interface.
This should be driven by real world use cases. For example:

1. Ignoring existing checkpoints
1. Start at the beginning or end of existing files, optionally taking into
   account factors like mtime
1. Start at the beginning or end of files added while we're watching (this can
   be tricky)
1. Ordering which of the above concerns take precedence

I would suggest a config like the following:

* `ignore_checkpoints = true|false` (still write but don't read existing checkpoints)
* `read_from = beginning|end` (where to start if there's no checkpoint or
    they're being ignored)
* `skip_older_than = duration` (relevant when `read_from = beginning`, seek to
    end based on mtime)
* Always start at the beginning of files added while we're watching (it's hard
    to tell a `mv` from a create and write, so don't rely on seeing an empty file
    first to get all the data)

If we adopt this, we can also implement a solution where we don't hold open file
handles for files that match `skip_older_than`, since this has caused some
issues. Naively, we need the open handle to attempt reads in case new data is
written to the file, but we could also implement that as a new state of
`FileWatcher` that stashes the initial size and periodically checks the file
metadata (mtime and size) to see if it should start reading at what was the end.

### General tweaks

In addition to extracting these various subcomponents, there are some other
relatively simple changes we can make to help address the issues we discussed.

#### File discovery and checkpointing

We currently use the outdated `glob_minimum_cooldown` config option to determine
how often to do both of these tasks. We should switch them to their own
independent config options and allow them to be disabled (e.g. a batch use case
does not need to continuously look for new files).

We should also move them into their own periodic tasks, outside of the main read
loop. This should give us better performance and help avoid bad behavior in
situations where either is expensive (e.g. discovering millions of files on
EBS).

#### Read concurrency

With most of the other concerns pulled out, we can afford to adjust the main
read loop to allow for some new capabilities. The most interesting is concurrent
reads, but it will require a bit of experimentation before we're able to
determine if it's worthwhile. There are a few possible approaches:

1. Dispatch reads to an explicit threadpool
1. Spawn a limited number of blocking tokio tasks
1. Implement something with `iouring`

The first two both introduce the questions of sizing and the ability of the
underlying file system to enable concurrent access in a way that actually adds
performance. We would need to test a wide variety of scenarios to evaluate the
best path for either.

Much more interesting, but also limited, would be to build on top of `iouring`.
Given that it's only available in modern Linux, it could not be the only
implementation. But since modern Linux is the majority of Vector usage, it could
be enough to cover the most demanding use cases. This approach would let us
avoid any questions of thread counts and defer concurrency to the kernel, which
is much better equipped to make use of the available hardware.

Given these options, I would currently propose that we wait. There are other
changes in this RFC that should impact performance positively, and we'll have
a better view of the potential cost/benefit of these approaches once those have
landed. When we reach that phase, I would suggest starting with the tokio
filesystem interface, as future improvements like `iouring` are likely to better
match the async interface.

#### Compression-aware fingerprints

As it stands, both `Fingerprinter` and `FileWatcher` instances read data from
files, one to checksum that data and the other to return lines. The problem is
that only `FileWatcher` handles compression, so our fingerprints can get
confused if a file is rotated and compressed.

To address that, we should evolve `FileWatcher` into a more general wrapper
struct for file handles. This will allow us to encapsulate all direct file
access to within the struct, where we can more easily ensure that concerns like
compression are handled uniformly.

#### Batch mode

In addition to our current shutdown logic, we should add a configuration option
and corresponding conditional for exiting the source once all files have reached
EOF. This, along with disabled file discovery, would neatly and simply implement
the oft-requested batch mode.

#### Observability improvements

There's no grand design here, but we should do the work to go through and
address all of the relevant issues. Some examples:

* Logging when we see that a file is deleted but need to keep it open
* Not logging noise around small or empty files
* Optionally silencing errors due to dangling symlinks
* Exposing which files are being read and our progress through them

For the last item, it would be particularly helpful to evolve how we store
checkpoints. Instead of the strange filename-based system we have now, we should
migrate towards a JSON file-based approach as laid out in
[#1779](https://github.com/vectordotdev/vector/issues/1779).

## Doc-level Proposal

* Rename `ignore_older` to `skip_older_than`
* Replace `start_from_beginning` with `ignore_checkpoints` and `read_from` (see
    earlier section)
* Replace `oldest_first` with `mode = tail|tail_sequential|batch`
* Rename `glob_minimum_cooldown` to `discovery_interval` and disable when `mode = batch`

## Rationale

The file source has dramatically overgrown its original design and is causing
pain for both users and developers of Vector. It warrants spending some time and
effort to improve both usability and maintainability. Otherwise, we risk losing
a large and increasing amount of our time to its maintenance and user support.

By focusing on modularity and simple improvements over a rewrite, we will be
able to maintain the good parts of the source's long history (accumulated bug
fixes). This also reduces the risk of introducing new bugs that would be
inevitable with a more aggressive rewrite.

Modularity also positions us well for the future by encouraging a split between
user-facing config and implementation-level config. This matches well with our
config composition RFC and should enable simple config facades for specific
file-based use cases.

## Drawbacks

This is a backwards incompatible change to the config of one of our most widely
used sources. To change the interface would likely inconvenience a large number
of current users. There is also always a risk that bugs are introduced as part
of refactoring, even though our approach is designed to minimize that risk.

## Alternatives

One alternative would be to rewrite the source from scratch. While this would
likely result in more maintainable code, it would risk losing much of the
accumulated knowledge present in the existing implementation. It would also
likely take much longer and present a more difficult transition plan than the
evolution proposed here.

Another alternative would be to leave the implementation largely alone and focus
on improving our documentation and config UI alone. While this would likely
yield strong benefits, it would leave a number of important issues unaddressed
and do little for our ability to extend the source in the future.

## Outstanding Questions

* Should we change the user-facing config at the same time at the implementation
    or split the two?
* How to handle transitioning users to the new interface?

## Plan Of Attack

The work with the highest investment/payoff ratio is likely around
checksumming, so I would suggest we attack that first:

* [ ] Migrate `FileWatcher` to general purpose file wrapper with fingerprinting
* [ ] Rework checkpoint persistence to allow differentiating and migrating
    between types
* [ ] Combine checksum and first line checksum fingerprinting strategies
* [ ] Add path-based fingerprinting strategy
* [ ] Deprecate device/inode fingerprinting strategy

The next most valuable is removing extraneous work from the read loop and
hopefully improving performance in some edge cases considerably:

* [ ] Move path discovery to its own task and interval
* [ ] Move checkpoint persistence to its own task and interval

With the relatively low hanging fruit taken care of, we can move on to the more
general reorganization tasks that set the stage for configuration improvements:

* [ ] Extract scheduler component
* [ ] Extract file identity component (depends on `FileWatcher` work)
* [ ] Extract file starting point component (depends on `FileWatcher` work)

Followed by the new and improved configuration itself:

* [ ] Implement `version = 2` of the file source config with deprecation warning
* [ ] Rename `ignore_older` to `skip_older_than`
* [ ] Replace `start_from_beginning` with `ignore_checkpoints` and `read_from`
* [ ] Replace `oldest_first` with `mode = tail|tail_sequential`
* [ ] Rename `glob_minimum_cooldown` to `discovery_interval`

Which in turn enable the new `batch` mode to be implemented and exposed:

* [ ] Implement batch mode shutdown conditions and new config `mode`

Any time after the new file wrapper work is done, we can improve it to stop
holding unnecessary file handles:

* [ ] Add state to file wrapper where file is tracked without an open handle for
    files that have been idle for a certain period of time

And finally, or in parallel with any of the above, go through and do the work to
smooth out observability warts:

* [ ] Log when file is no longer findable but watcher isn't dead
* [ ] Don't log on empty files
* [ ] Add option to disable logging on dangling symlinks
