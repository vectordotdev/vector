# Development Best Practices

This guide captures the core practices for contributing to Vector.

## Commit Messages

Follow the guidelines in [AGENTS.md](../../AGENTS.md) for conventional commit format. Include a reference to the related GitHub issue.

## Test Driven Development

Add unit tests for new features or bug fixes. Python components should be tested with `make test-python`. Mock external services to keep tests fast and deterministic.

## Debugging and Observability

Use rich logging and metrics during development to make features observable in production. Document any debugging steps or scripts created to help others reproduce issues.

