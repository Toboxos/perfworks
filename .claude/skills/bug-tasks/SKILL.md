---
name: bug-tasks
description: Use when the user reports a bug, requests a feature, or discusses the design of one in the perfwork repo — to file it in tasks/, look up an existing task, or append a design decision to one. Covers the tasks/ ticket format and its append-only discussion log.
---

# Bug and design tasks

`tasks/` holds one markdown file per bug report or feature request, each acting like a
ticket: a fixed report at the top, then an append-only log of design decisions below.

**`tasks/` is in `.gitignore` and must stay that way.** It is a local scratchpad, not
repository history. Never `git add` it, never commit it, and never suggest tracking it.

## When to use this

- The user reports a bug or requests a feature → **file it** (see below).
- The user discusses an approach, rejects an option, or settles a question about work
  already filed → **append to the Discussion** of that task.
- Starting work on something that sounds familiar → **read `tasks/` first**; the decisions
  already taken are there, and re-deriving them wastes the user's time.

Filing a report is not an instruction to fix it. Do not start implementing unless the user
asks. If they report several bugs at once, file all of them before touching any code.

## Filing a new task

Create `tasks/NNNN-kebab-summary.md`, where `NNNN` is the next unused four-digit number
(`ls tasks/` to find it). Keep the slug short and about the symptom, not the suspected
cause — the cause is often wrong on first contact.

```markdown
# NNNN — One-line summary in the present tense

- **Status:** open
- **Area:** <modules likely involved, e.g. canvas, model, netlist>
- **Reported:** YYYY-MM-DD

## Bug report

<The report, in the user's own terms. Preserve their examples, numbers and reproduction
steps. Keep their suggested solutions, but under their framing — "Suggested: ..." — so a
later reader can tell a symptom from a proposal.>

## Discussion

<!--
Append design decisions, options considered and rejected, and the reasoning behind the
final choice. Treat this like the comment thread on a ticket: newest at the bottom, each
entry dated. Do not rewrite earlier entries — supersede them.
-->
```

Write the **Bug report** section from what the user actually said. Do not silently correct
their diagnosis, merge two reports that sound similar, or drop a detail because it looks
irrelevant — in this codebase the reported symptom has repeatedly been the accurate part
and the guessed cause the wrong one.

## Appending a decision

Add a dated entry at the bottom of **Discussion**. Never edit or delete an earlier entry;
if it turns out to be wrong, write a new entry saying so and why. The value of the log is
that it shows what was believed when, including the mistakes.

```markdown
### 2026-11-18 — Chose X over Y

Y looked simpler but <reason it fails>. X costs <cost> and buys <benefit>.
Rejected Z entirely because <reason>.
```

Record the reasoning, not just the outcome. An entry that says only "decided to use X" is
worth almost nothing six weeks later.

Keep `Status` current: `open` → `in progress` → `done` (or `wontfix`, with an entry saying
why). When a task is finished, leave the file in place; the log is the point.

## Cross-cutting notes

Several open tasks may share a root cause — a structural limit rather than four separate
defects. When that is apparent, say so in each affected task's Discussion and link them by
filename, rather than quietly fixing one and leaving the others to resurface.
