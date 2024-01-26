# RFCs

Vector uses the RFC process to formalize discussion around _substantial_ changes to Vector.

- [Goals](#goals)
- [Logical boundary](#logical-boundary)
- [Tenets of good RFCs](#tenets-of-good-rfcs)
  - [Use good technical grammar](#use-good-technical-grammar)
  - [Keep the scope small](#keep-the-scope-small)
  - [Be opinionated and propose a single solution](#be-opinionated-and-propose-a-single-solution)
- [Process](#process)
  - [Before creating an RFC](#before-creating-an-rfc)
  - [Creating an RFC](#creating-an-rfc)
  - [Getting an RFC accepted](#getting-an-rfc-accepted)
  - [Implementing an RFC](#implementing-an-rfc)
- [FAQ](#faq)
  - [What if I'm unsure about the solution?](#what-if-im-unsure-about-the-solution)
  - [What if I need to investigate and feel out solutions?](#what-if-i-need-to-investigate-and-feel-out-solutions)
  - [How long should it take to obtain consensus?](#how-long-should-it-take-to-obtain-consensus)

## Goals

- Properly spec and plan features to prevent re-work
- Formalize discussion to optimize everyone's time
- Obtain consensus upfront, before implementation time is invested
- Incorporate project-wide context and leverage our shared brain
- Share responsibility for the outcome of the change
- Benefit posterity and facilitate new Vector team member onboarding

## Logical boundary

Examples of changes that require a RFC:

- An architectural change
- A data model change
- A new component that introduces new behavior
- Removing a feature
- Complicated tech-debt projects
- A substantial user-visible change
- A change that is questionably outside of the scope of Vector

Examples of changes that do not require a RFC:

- Reorganizing code that otherwise does not change its functional behavior
- Quantitative improvements. Such as performance improvements
- Simple improvements to existing features

## Tenets of good RFCs

### Use good technical grammar

1. Use a factual tone. Convey subject matter in a clear, concise, and confident manner. Avoid using vague language such
   as “it seems” or “probably.”
2. Use present tense. Instead of "this RFC was created to describe...", say "this RFC describes".
3. Structure your RFC with headings that address one point at a time. This is necessary to facilitate productive
   discussion via inline comments.
4. Start with "why". Lead with one or two sentences describing the higher level intent. Leading with "how" omits the
   opportunity to consider alternative approaches.

### Keep the scope small

Use the "Scope" section to address out of scope concerns, like future improvements. This signals to the Vector team
that you are aware of these concerns but have explicitly chosen to defer them. This not only helps to keep the RFC
discussion focused, but also the implementation, resulting in a quicker overall delivery time.

### Be opinionated and propose a single solution

Your job as the RFC author is to navigate the issue at hand and propose an opinionated solution. Ambiguity creates
unproductive discussion and should be eliminated while drafting the RFC. This does not mean you need to go at it alone.
You are encouraged to investigate and discuss solutions with the Vector team while you draft the RFC, but the end state
of the RFC should land on a single recommendation that you are reasonably confident in. Use the "Rationale" section to
justify your proposal and the "Alternatives" section to note other solutions you considered. See the [FAQ](#faq) for
more info navigating your solution.

## Process

### Before creating an RFC

1. Search GitHub for [previous issues](https://github.com/vectordotdev/vector/issues) and
   [RFCs](https://github.com/vectordotdev/vector/tree/master/rfcs) on this topic.
1. If an RFC issue does not exist, [open one](https://github.com/vectordotdev/vector/issues/new/choose).
1. Use the issue to obtain consensus that an RFC is necessary.
   - The change might be quickly rejected.
   - The change might be on our long term roadmap and get deferred.
   - The change might be blocked by other work.

### Creating an RFC

1. Create a new branch
1. Copy the [`rfcs/_YYYY-MM-DD-issue#-title.md`](rfcs/_YYYY-MM-DD-issue%23-title.md) template with the appropriate
   name. Be sure to use the issue number you created above. (e.g., `rfcs/2020-02-10-445-internal-observability.md`)
1. Fill in your RFC, pay attention the bullets and guidelines. Do not omit any sections.
1. Work with the Vector team to land on a confident solution. Allocate time for code-level spikes if necessary.
1. Submit your RFC as a pull request and tag reviewers for approval.

### Getting an RFC accepted

1. Schedule a "last call" meeting for your RFC. This should be 1 week after opening your pull request. The purpose is to efficiently obtain consensus.
1. At least 3 Vector team members must approve your RFC in the form of pull request approvals.
1. Once approved, self-merge your RFC, or ask a Vector team member to do it for you.

### Implementing an RFC

1. Create issues from the "Plan Of Attack" section. Place them in an epic if necessary.
1. Coordinate with leadership to schedule your work.

## FAQ

### What if I'm unsure about the solution?

Your project should be assigned 1 or more "reviewers" (AKA domain experts). Leverage these people to navigate the
appropriate path forward; don't be afraid to involve other Vector team members if necessary. Generally, real-time
chats are the most productive way to identify a path forward, and code-level spikes are much more effective at
demonstrating intent over conceptual discussions.

### What if I need to investigate and feel out solutions?

This is expected. RFCs are the time to discuss with the Vector team and experiment with code-level spikes. It is not
uncommon for RFCs to span one or two sprints, but they should not take longer than two sprints. Generally, if an RFC
takes two sprints it involves many cross cutting concerns that result in incremental RFCs.

### How long should it take to obtain consensus?

Barring any substantial changes due to feedback, it should not take longer than a week to obtain consensus. For most
RFCs it should only take a few days if reviews are timely. Please nudge your reviewers if they have not reviewed
your RFC in 48 hours.
