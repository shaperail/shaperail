Implement SteelAPI milestone $ARGUMENTS.

Before writing any code:
1. Read agent_docs/milestones.md — find section M$ARGUMENTS, read every deliverable
2. Read agent_docs/architecture.md — confirm which crate(s) this milestone touches
3. Read the relevant agent_docs/ file for this area
4. Read agent_docs/docker.md if the milestone involves server startup or CLI commands
5. State your implementation plan in 3-5 bullet points before starting

Implementation rules:
- Work through deliverables in listed order — do not skip ahead
- After EVERY deliverable: `cargo build -p <crate>` must pass before continuing
- Check off each item in milestones.md as you complete it: [ ] → [x]
- The resource file format must match agent_docs/resource-format.md exactly
- Never use `.unwrap()` or `.expect()` outside of test code
- Never use raw `query()` — always `sqlx::query_as!` macro
- If ambiguous, ask ONE clarifying question before proceeding

When all deliverables are done:
1. Run /run-checks — fix everything until it prints "✅ All checks passed"
2. Mark M$ARGUMENTS as [x] Complete in agent_docs/milestones.md
3. Update agent_docs/current-milestone.md to reflect completion
4. Output the git commit message: `feat(<crate>): M$ARGUMENTS — <milestone name>`
