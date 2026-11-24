---
name: mod-design
description: Build or update the design wiki of this mod, the HTML pages of `docs/design/` on the `design` branch (a tree: the home page `index.html`, a page per feature, sub-pages, each saying what happens in game before how it is built), from the design documents next to them (`docs/design/**/*.md`), as self-contained pages that developers read without Claude, published by GitHub Pages. Use when the design documents changed and the pages must follow, when asked to build, refresh or rework the design, or when someone finds it unclear or thin ("too much text", "I cannot see how the sides talk", "it lacks detail", "split it into pages", "everything is at the same level", "mixes functional and technical"). Not for writing the design itself, which happens in the Markdown documents.
---

# Design wiki of the mod

The `design` branch holds the design of the mod twice:

- the Markdown documents of `docs/design/` and its folders, for agents. They
  are the source;
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

The wiki is a tree, not a flat list:

- **The home page** (`index.html`, from `00-overview.md`) stays at the level
  of the mod: what it adds and what the player does with it, one diagram of
  the whole, its features with one line each, the glossary, and the open
  questions with a link to each.
- **One page per feature** (`<feature>/index.html`, from
  `<feature>/00-overview.md`): a part of the mod a player can name (a mob, a
  mechanic, a block family, a system such as ownership), at the level of the
  feature: what it does in game, one diagram of how its parts fit, and its
  sub-pages. A feature not designed yet still gets its page, saying what is
  already known.
- **Sub-pages** (`<feature>/<subject>.html`, from `<feature>/NN-<subject>.md`)
  go into the detail of one part of the feature: how it works step by step,
  every decision with its reason and the options ruled out, the edge cases,
  what is still open. A sub-page too big splits again, one level down.
- **Pages for the whole mod** (sides, config, blocks and items) sit at the
  top level, next to the features, and link to the features for their detail.

The folders of `docs/design/` follow the tree, Markdown documents and HTML
pages side by side. Every page carries the same navigation: the whole tree
(a side column on a wide screen, a collapsible block on a phone, with
`<details>`, no script), with the current page marked, and a breadcrumb
(Home › Feature › Sub-page). A link points to the page that holds a subject
rather than repeating it: one subject, one place.

## In game first, then how it is built

A design mixes two things: what the mod does in game, and how it is built.
Each page separates them, in that order:

- **In game** comes first: what the player sees and does, the rules, the
  states, as a player or a designer would describe them, with no class name.
- **How it is built** comes after: the classes, events, packets and data
  that make it happen, and the vanilla or library code it relies on.

The two parts are marked as such on the page (headings and a visible
label), so a reader always knows which one they are in. A page with nothing
in game (how the client shows an entity, how data is saved) is a technical
page and says so under its title; a page of a feature never starts with the
technical part.

In the documents, the same split: `## In game`, then `## How it is built`,
each with its own sub-headings.

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

- **One page per feature**, with its sub-pages: its diagrams, its rules,
  what triggers it and what it changes.
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
`find docs/design -name '*.html' -exec sh <this skill's directory>/check-page.sh {} +`.
It lists everything a page loads; fix each one until it says every page is
self-contained.

## Steps

1. Read every document of `docs/design/`, and the current pages if they are
   not the starter one.
2. Plan the tree: the home, the features and their sub-pages, the pages for
   the whole mod, one document each. When a document holds several subjects,
   or mixes a feature with the whole mod, split it first.
3. Write the pages. When they exist, keep what still holds and rework around
   it; a diagram that is no longer true goes, or changes with the decision
   behind it.
4. Run `check-page.sh` on every page until it passes.
5. Look at every rendered page when a browser is available: desktop and phone
   width, light and dark theme.
6. For a first version or a rework, have a fresh agent read the wiki cold,
   with only the path of the home page and who the reader is: what it
   understood from the home page alone, whether it found the page of a given
   part through the tree, whether it could tell what happens in game from how
   it is built, whether it could build that part from its page, and what is
   unclear, missing, too long or badly displayed. It changes nothing; you decide what to fix.
7. Commit on `design` and push it. Pages publishes the wiki within minutes.
   When `origin` has no `design` branch yet, do not push it: on an empty
   repository it would become the default branch. `mod setup` pushes it once
   the version branch is there, and sets Pages.
