# AI Policy

We use AI tools ourselves and encourage their use. This policy isn't about limiting how you work. It's about keeping contributions high-quality and reviews sustainable for a small team maintaining a popular open source project that receives a lot of PRs.

## New to Vector?

LLMs are genuinely great for onboarding into a large codebase. If you're just getting started, ask one to explain a component, trace a data flow, or summarize what a module does. It can save you a lot of time getting oriented.

That said, there's still no substitute for some good old-fashioned reading. Spending time in the actual source, following the logic yourself, and running things locally is what builds the real intuition you'll need to contribute confidently. Use the LLM as a guide, as a helper.

## You own what you submit

When you open a PR or issue, you're vouching for it. Ideally that means you understand what the changes do, why they're correct, and how they fit Vector's architecture and conventions. If a reviewer asks you a question, please answer it in your own words rather than relaying AI output.

This applies whether you wrote the code yourself, used AI as a coding assistant, or anything in between. The goal is the same: be the expert on your own contribution.

## Keep conversations human

GitHub discussions (issues, PR descriptions, review threads) are how our community thinks together. Honestly, if a response is thoughtful and on point, does it matter whether a human wrote every word? We don't think so. What we'd rather avoid is raw AI output forwarded into a thread without review — that's not engaging with feedback, it's delegating it.

## Quality over volume

AI tools can generate a lot of code quickly. Please keep PRs well-scoped and focused: one clear problem, one clear solution. Our reviewers are humans with limited time, and a focused, well-tested PR that solves a real problem will get reviewed and merged faster.

Before opening a PR, it helps to make sure it addresses a real need (ideally tied to an open issue), that your changes are tested, and that you've followed the standard checks described in [CONTRIBUTING.md](CONTRIBUTING.md#running-other-checks).

## AI review comments

Pull requests often receive automated AI code review. Please take a moment to go through those comments before requesting a human review. If a comment doesn't apply, please include a brief note when dismissing it. Resolving comments without any explanation creates friction for human reviewers, who then have to scan each one and figure out whether a follow-up commit addressed it or it was simply closed without consideration.

We'd also love if you could like or dislike each AI comment. It only takes a second and it genuinely helps us understand whether the tool is pulling its weight. So far the signal has been really good and the vast majority of comments have been valid, so your feedback helps us keep measuring that.

## Agentic contributions

Agentic PRs aren't a special category. They're held to exactly the same bar as any other contribution: the code should be understood by the person submitting it, covered by unit tests, and validated with integration or end-to-end tests where appropriate. If you're using an agent to help you build something, that's great. We'd just ask that you've read, understood, and tested what it produced before submitting.

---

That's it. If something isn't covered here, the underlying principle is: please be considerate of reviewers' time, and take ownership of the work you submit. We really appreciate your contributions.
