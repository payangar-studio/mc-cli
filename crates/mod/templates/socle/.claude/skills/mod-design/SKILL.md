---
name: mod-design
description: Build or update the design wiki of this mod, the HTML pages of `docs/design/` on the `design` branch (home page `index.html`, one page per subject), from the design documents next to them (`docs/design/*.md`), as self-contained pages that developers read without Claude, published by GitHub Pages. Use when the design documents changed and the pages must follow, when asked to build, refresh or rework the design, or when someone finds it unclear or thin ("too much text", "I cannot see how the sides talk", "it lacks detail", "split it into pages"). Not for writing the design itself, which happens in the Markdown documents.
---

# Design wiki of the mod

The `design` branch holds the design of the mod twice:

- the Markdown documents of `docs/design/`, for agents. They are the source;
- the HTML pages next to them, the same design as a wiki for developers.
  GitHub Pages serves it at `https://<owner>.github.io/<repository>/design/`,
  whose home is `index.html`.

The pages show what the documents say, nothing more. When they disagree, the
pages are wrong. The branch does not follow the code: it says what the mod is
meant to do.

Both are written in English, like the documents `mod init` starts with.

## Where to work

The branch is checked out apart from the code. Find a worktree of it with
`git worktree list`; if there is none, add one next to the repository:
`git worktree add ../<repository>-design design`. Work there, and never merge
`design` into a version branch or the other way round.

`mod init` creates the branch. When there is none, here or on `origin`
(`git branch -a --list design '*/design'` lists nothing), create it as an
orphan: `git worktree add --orphan -b design ../<repository>-design`. Then
add an empty `docs/.nojekyll` and the documents of `docs/design/`, starting
with `00-overview.md`, and build the pages from them. Git refuses `design`
beside branches named `design/...`: rename those first.

## A wiki for the developer

The reader is a developer who will build a part of the mod, or come back to it
months later. They have not followed the conversations that produced the
design. They open the home page, understand the mod at a glance, then go down
into the page of the part they work on, and build it from that page alone.

- **The home page** (`index.html`, from `00-overview.md`) stays at the macro
  level: what the mod adds and what the player does with it, one diagram of
  the whole (the main parts and how they connect), the glossary, the map of
  the pages with one line each saying what it answers, and the open
  questions with a link to each.
- **One page per subject** (`<subject>.html`, from `NN-<subject>.md`): a
  mechanic, a system, the sides, saving, the blocks and items, the config.
  It goes into the detail: how it works step by step, every decision with its
  reason and the options ruled out, the edge cases, what is still open. A
  subject too big for one page splits into several.
- **Every page** has the same navigation at the top: the home, then the other
  pages. A link points to the page that holds a subject rather than repeating
  it: one subject, one place.

Detail is what makes the design buildable. A design that says what without
saying how leaves the developer to rediscover the how: show the mechanism
behind each decision (which parts take part, what happens in which order,
what is saved where and when), at the level of principle, without code beyond
the few lines that make a point.

## What a page is made of

A page is looked at before it is read: its diagrams and headings give its
subject at a glance, then the text gives the detail where one looks for it.

Pick the form from what is described, not by habit:

| What the documents describe | Form on the page |
|---|---|
| A whole made of parts that depend on each other | An inline SVG diagram of the parts and their links |
| Something that happens in steps, over time or across the sides (a switch, a save, packets) | A sequence diagram: one lane per actor (server, client, player...), time going down, one arrow per step |
| Something with states and transitions (a lifecycle) | A state diagram: one box per state, one arrow per transition, labelled with its trigger |
| Comparable things: blocks, items, entities, config keys, commands | A table, one short line per row and why it exists |
| What only words can carry | A short paragraph |

A paragraph that explains what a diagram would show means the diagram is
missing. Every mechanism the design relies on has its diagram, including the
ones that only matter to the developer (how a client shows a server entity,
how data crosses from one side to the other).

A diagram is understood without someone pointing at it: what links the parts
shows more than the parts. It carries numbered marks, and a numbered legend
below it explains each one. A concrete example ("a zombie hit by the blade at
night loses 4 hearts") shows a rule better than its wording.

Pages a mod usually has, besides the home:

- **One page per mechanic**: its diagrams, its rules, what triggers it and
  what it changes.
- **Blocks and items**: a table with id, what it does, how it is obtained
  (recipe, loot, trade), and its notable properties. A crafting recipe is
  drawn as the 3x3 grid of the game. A texture of the mod may appear as a
  `data:` PNG with `image-rendering: pixelated`.
- **Config**: a table with key, default, range, side (client or server) and
  effect.
- **Sides**: what runs on the client, what runs on the server, and what
  crosses between them (packets, synced values), with a diagram. `mod.toml`
  says where the mod runs (`side`); the pages use the same words.

Also:

- A glossary on the home page when the mod brings its own words, each word
  then used in one sense only on every page. A name not chosen yet is shown as
  missing ("???"), never replaced by a working word.
- A statement says what it is: a fact, a decision or a question. A decision
  gives its reason. Options are shown at the same level of detail, without a
  recommendation; once one is chosen, the others go to a "ruled out" table with
  why.
- An open question sits on the page where its problem is described, with what
  each answer would change.

## Each page is self-contained

The wiki must read offline, from Pages, or from a copy of the folder:

- styles in one `<style>` per page, scripts (if any) in `<script>`, diagrams
  as inline SVG, images as `data:` URIs;
- no CDN, no web font, no remote image, no `fetch`, no file besides the pages
  themselves, and nothing that needs Claude to render (no artifact runtime);
- links people follow (`<a href>`) are fine, between pages too: they load
  nothing;
- colors as CSS variables on `:root`, redefined under
  `@media (prefers-color-scheme: dark)`, with an explicit `body` background;
  the same palette and layout on every page;
- readable on a phone: a 16px side gutter, no horizontal scroll of the page.
  A wide table and a diagram each scroll in their own box; a diagram keeps the
  minimum width at which its labels stay legible instead of shrinking to the
  screen.

Check them, from the worktree of `design`, with the script next to this skill:
`sh <this skill's directory>/check-page.sh docs/design/*.html`. It lists
everything a page loads; fix each one until it says every page is
self-contained.

## Steps

1. Read every document of `docs/design/`, and the current pages if they are
   not the starter one.
2. Plan the wiki: the home and one page per subject, one document each. When a
   document holds several subjects, split it first.
3. Write the pages. When they exist, keep what still holds and rework around
   it; a diagram that is no longer true goes, or changes with the decision
   behind it.
4. Run `check-page.sh` on every page until it passes.
5. Look at every rendered page when a browser is available: desktop and phone
   width, light and dark theme.
6. For a first version or a rework, have a fresh agent read the wiki cold,
   with only the path of the home page and who the reader is: what it
   understood from the home page alone, then whether it could build a given
   part from its page, and what is unclear, missing, too long or badly
   displayed. It changes nothing; you decide what to fix.
7. Commit on `design` and push it. Pages publishes the wiki within minutes.
   When `origin` has no `design` branch yet, do not push it: on an empty
   repository it would become the default branch. `mod setup` pushes it once
   the version branch is there, and sets Pages.
