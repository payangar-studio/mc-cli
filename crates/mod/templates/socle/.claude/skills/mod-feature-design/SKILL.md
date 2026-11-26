---
name: mod-feature-design
description: Design a feature of this mod with the user, from the idea to the issue of its first brick, before any code. Use when the user has an idea or a need for the mod ("design a feature", "I have an idea for the mod", "how should we build X", "concevoir une fonctionnalité", "j'ai une idée de fonctionnalité"), or when a request needs a design before it can be coded. Not for building the design wiki from the documents (that is mod-wiki-design) and not for coding.
---

# Designing a feature

The user brings an idea; this skill turns it into decisions that hold against
the real game and the mods around it, then into the issue of a first brick.
It runs as a dialogue in the main conversation: subagents cannot talk to the
user, so the research goes to them and the decisions stay here.

Keep the main context lean: reading code, files and logs goes to subagents,
the `mod-research` agent for questions about Minecraft or mods code, several
in parallel when the questions are independent. What comes back is a
conclusion with its evidence, which you check before building a decision on
it.

The design lives on the `design` branch, in the Markdown documents of
`docs/design/`; the `mod-wiki-design` skill says where to work on that branch
and how the documents are laid out.

## 1. The need

Dig until the feature is described in game terms: what the player sees, does
and expects, the rules, what must never happen. Separate the vision (where the
feature may go) from the first brick (what gets built now); the user owns both.
A requirement stated as a solution ("make it a player") is traced back to the
need behind it ("it must animate like a player").

Done when the user recognises the need written back to them, and the first
brick is named.

## 2. The mods to keep working

The design home document, `docs/design/00-overview.md`, holds a "Mods to keep
working" table: the mods the feature must stay compatible with, with no
compatibility code where possible. When the table is missing or empty, ask the
user once which modpack or mods matter, and record them there.

Done when every mod the feature touches is listed, with what it does to it.

## 3. Research

Every question about how the game or a mod behaves is answered from the code
of the targeted Minecraft version and of the mods, never from memory. A fact
carries its `file:line`; what is inferred is marked as such, and stays a
question until checked. What can be investigated is investigated, not asked:
"is this a problem?" is a research question, not a question for the user.

Done when every assumption the options rely on is either verified or flagged.

## 4. Options and their stress test

Lay out several architectures at the same level of detail, then stress each
against vanilla mechanics and the edge cases: what runs on which side, saving
and crash windows, death, riding, dimension change, network sync and entity
tracking order, identity and scoreboard names, the hooks of the mods to keep
working. Present them neutrally; an opinion comes after, labelled.

Principles that decide between options, and why:

- **Separate client and server needs.** What is visual is decided on the
  client, so the server entity need not be what the client shows. Most
  rendering or animation constraints vanish on the server once this is seen.
- **Compose capabilities one by one,** each through a vanilla mechanism the
  mod drives (its own chunk ticket, vanilla's death message), rather than
  inheriting a whole native behaviour (a fake player). Inherited behaviour
  brings everything else with it, and each unwanted part needs a workaround.
- **Refuse workarounds and hidden helper entities.** When workarounds pile up,
  step back: the base is wrong, and patching it further only hides that.
- **Needing a mixin is a smell.** It usually means the design is working
  around something. Before keeping one, look around: is it really the only
  way, is it the best one, would another base or another vanilla mechanism
  remove the need? Mixins planned to hide a player from Tab or to keep a client
  entity across a swap all vanished once the base was right. One that survives
  is surgical, and its reason is written in the design.
- **Only real decisions go to the user,** with what each answer changes. A
  principle the user already gave ("like a vanilla variant") is applied, not
  asked again.

Done when one option is chosen by the user, and the others are recorded as
ruled out with their reason.

## 5. Spike, when an assumption is risky

When the chosen option rests on something only the game can confirm (a mod
animating an entity, a client accepting another class), prove it before
designing further: a throwaway branch, a gametest that probes the assumption,
then the real mods in game with `mod run` (plus resource packs when they carry
the behaviour). The spike is not merged; what it proved becomes a fact in the
design.

Done when the risky assumption is confirmed or the option is dropped.

## 6. Record as it goes

Decisions land in the design documents as they are taken, laid out as
`mod-wiki-design` describes: the feature's folder, in game first, then how it
is built, each statement tagged as a fact, a decision or a question. The
design is the reference; the conversation is not. Once the documents hold the
design, build the wiki pages from them with `mod-wiki-design`.

The last step is the issue of the first brick: scope, out of scope, and the
proof expected (the tests, and what is checked in game). Write them in prose,
without task lists: the checklist belongs to the draft pull request that
builds the brick, as `AGENTS.md` says. Show the issue to the user before
writing it to GitHub.

Done when the design and the issue agree, and the user has the issue.
