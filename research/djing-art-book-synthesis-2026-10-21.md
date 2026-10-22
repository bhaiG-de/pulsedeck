# Rebuilding DJing — the art from the two books

Prepared 21 October 2026. Scope: replace the musical skill in full while preserving the working deck-control/library workflows and the existing read-only phrase checker.

## What the books change

The most consequential shared lesson is a hierarchy of decisions: choose music for the moment, choose where the records should meet, then perform that relationship convincingly. Beatmatching is essential for many overlaps, but it cannot decide the first two questions. This directly addresses the reported gap between reliable CLI performance and weak DJ judgment.

Morse is especially useful for separating tempo matching, beat alignment and phrase matching; preparing a viable next record early; distinguishing private preparation from audible introduction; and keeping a small vocabulary of dependable transitions. His programming chapters explain why an easily mixed record can still be the wrong next record.

Broughton and Brewster give the stronger treatment of mix placement as musical editing: complete statements, introductions of different lengths, tightening the distance between the songs' main ideas, preserving run-up notes, and using arrangement changes to control the set's energy. Their rhythm, programming and pacing chapters also make clear that equal BPM is not the same thing as a compatible groove, and that a whole night cannot be understood as an uninterrupted succession of peaks.

The exact chapter/resource map is in the skill's [sources](../.agents/skills/djing-art/references/sources.md). Both EPUBs were opened and their relevant musical chapters examined. Gear buying, career promotion and industry advice were outside the musicianship rewrite. Extracted book text stayed in a temporary study directory; the skill contains original synthesis rather than copied chapters.

## Diagnosis of the previous skill

The old skill already contained many correct cautions, including backward counting and the limitations of numerical analysis. The problem was not a complete absence of theory. It offered a largely optional reference library and illustrative techniques, with the central musical decisions scattered across it. An agent could operate fluently, select a plausible BPM/key pair and complete a checked timeline without resolving what either record was saying.

The previous entrypoint also foregrounded artistic freedom and instructor studies before giving a compact reusable decision procedure. Freedom remains valuable. The replacement supplies concrete decision obligations while leaving repertoire, timing scale and technique open to the brief.

Adding more genre recipes would not resolve that structural weakness. Nor would universal rules such as “always mix 16 bars,” “never leave before the outro,” or “only choose neighboring keys.” Each can produce a neatly executed, musically unsuitable set.

## The replacement's central decisions

1. **Selection:** what job should the next record do, and what musical link or contrast justifies it?
2. **Outgoing statement:** what should A finish, and what would be lost by leaving now?
3. **Arrival and launch:** what must arrive in B when A changes, and how far back is its complete entry?
4. **Composite arrangement:** who supplies foreground, foundation and support at every important event?
5. **Pulse and pitch:** do local clocks, metrical identity, groove and sounding harmony support this exposure?
6. **Evaluation:** did the actual join and following passage achieve that purpose?

These decisions are the entrypoint. References supply the detailed concepts when needed, rather than requiring an agent to reread the whole manual on every handover. The internal plan stays compact during performance; the user does not need a checklist transcript between songs.

## Concrete differences in behavior

**Phrase placement becomes an arrival problem.** If A has 32 bars to a target and B needs 24, simultaneous section starts make B arrive eight bars early. The agent must find a supported eight-bar offset, another complete entry, a different target or an intentional extension. Matching downbeats alone cannot answer this.

**Bass and foreground have separate timelines.** If A loses bass while its voice continues, B may need to supply foundation before it takes over the foreground. A single final fader exchange can leave a low-end hole or competing singers.

**Groove is evaluated beyond the grid.** Fixed phase error, growing rate error, wrong bar identity and incompatible subdivisions have different causes. Repeated nudging is an inadequate response to all four.

**Musical content has provenance.** A cue or actual listening can support a claim about a vocal ending. Waveform bands cannot. The agent must obtain the decisive evidence or use an approach requiring fewer unsupported assumptions. This avoids converting coarse analysis into fictional musical certainty.

**Set development is multidimensional.** Tempo, drive, intensity, tension, emotional color and familiarity differ. A spacious groove can refresh a dense set without raising BPM, and a quiet build can carry high tension. A library score does not specify a crowd's response.

**Technique serves the requested result.** A cut remains appropriate for a decisive contrast. A smooth-blend request requires meaningful audible overlap; muted pre-roll plus an instantaneous swap does not become a blend because the timing check passes.

## What was qualified rather than inherited

The books use simplifying language and reflect their periods. The rewrite does not adopt fixed phrase lengths, categorical demographic advice, guaranteed harmony from a key label, a requirement for perfect pitch, automatic emotion from major/minor, or a universal tempo correction limit. It distinguishes swing from syncopation and digital full scale from analog nominal level. It preserves explicitly planned sets rather than turning advice against rigid live playlists into a ban on choreography.

It also leaves artistic risk possible. A deliberate interruption, unusual meter, harmonic contrast, effect or loop can serve a performance. Its musical consequence needs an explanation and a supported execution path, not a blanket prohibition.

## Why this is an agent skill rather than a book summary

[OpenAI's skill guidance](https://developers.openai.com/plugins/build/skills) recommends focused workflows and supporting resources for detailed procedures. The rewrite uses a short discovery description, one shared decision process, and references routed by the current musical question. Software control remains in the sibling skill so improvements to judgment do not duplicate or destabilize the reliable CLI procedure.

[OpenAI's guidance on rethinking skills](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra) argues for revisiting accumulated instructions and avoiding bloated or contradictory discovery. The old artist/instructor study material was replaced rather than layered underneath a second competing manual. Earlier research remains historical context in the repository.

[OpenAI's skill evaluation guide](https://developers.openai.com/blog/eval-skills) distinguishes observable outcomes and qualitative assessment. The rewrite therefore has counted teaching examples, separate read-only evaluation prompts, technical validation, and a separate listening rubric. Skill syntax is not a proxy for musicianship.

## Validation and remaining evidence

Completed for the rewrite:

- Repository skill validation: three entrypoints, local links and 33 shell/JSON examples parse.
- Package tests: seven pass, covering relocation, link closure, overwrite protection and failure cleanup.
- Existing phrase-check tests: nine pass. The helper implementation was preserved.
- Skill-creator frontmatter/scaffold validation passes, using an isolated `uv` environment for its PyYAML dependency.
- The fourteen evaluation scenarios were initially reviewed for arithmetic/evidence requirements. Subsequent independent Luna trials and fixes are reported in [Luna trials](djing-art-evals/luna-trials-2026-10-21.md).

The [evaluation prompts](djing-art-evals/prompts.json) and [rubric](djing-art-evals/rubric.md) deliberately use different arrangements from the eight teaching cases. Independent Luna reasoning trials have now been run; the [trial report](djing-art-evals/luna-trials-2026-10-21.md) distinguishes the written results from the actual café performance and listening limitations. An old/new comparative listening trial remains a separate measurement. Record phrasing, groove, foreground clarity, bass/body continuity, level, payoff and direction separately from execution reliability.

The initial rewrite involved no live playback, recording or private-library edits. A subsequent authorized café performance is documented in the Luna trial report; recording and private-library edits remain outside that trial. There is no claim from written scores alone that the replacement sounds better. The report of two-to-three-hour CLI reliability is accepted as the user's observation; it was not independently retested. A new skill can improve the decisions asked of the agent, but cannot create a listening channel, reliable section/vocal evidence, track familiarity or audience feedback that the current tools do not supply.

## Local installation

The repository source and installed Codex `djing-art` copy were updated. The previous installed art skill was preserved at `/Users/yashwinghosh/.codex/pulsedeck-skill-backups/20261021-djing-art-98688ca0e9/djing-art`. Installed control/library skill contents were verified unchanged. All installed art links resolve within the installed sibling package. The application bundle itself was not rebuilt.

## Tempo feedback correction — 22 October

The listener reported that tempo changes made an incoming song sound wrong and that a handoff failed. The saved café-fun state corroborates a substantial stretch: Figure A (Tim Atlas, NASAYA) has source analysis 94.003 BPM, local pulse 94.496, actual playback 107.770, effective rate 1.140471 (about +14.05%) and key lock on. The performer reports canceling a later 80 → 107.09 BPM proposal (about +33.9%) before launch; the outgoing track then ended during troubleshooting. Distinguish the played stretch, the canceled proposal, and the recovery gap rather than treating them as one native tempo failure. These observations do not establish a DSP defect.

Reopened the supplied books: Morse, text/part0023.html, explicitly recommends close natural tempos and roughly ±4% for ordinary beatmixes; text/part0024.html permits genre/tempo changes through fade, end-to-end and cut techniques. Broughton/Brewster chapter 8 distinguishes momentary phase adjustment from persistent rate setting; chapters 15/31 warn that roughly ±3% already changes sounding pitch without key lock. Chapters 21/24 describe gradual programming and purposeful resets rather than mandatory acceleration.

The previous synthesis removed universal percentage bans but also lost Morse's useful selection default. The updated entrypoint now requires a source-relative rate check before arming; the beatmatching reference retains roughly ±4% as an ordinary starting neighborhood, requires listening support for wider character changes, covers inherited/cumulative rates and solo continuation, distinguishes nudges/gradual movement/resets, and rejects prolonged unsynced competing grooves as a substitute for an actual reset. Large expressive transformations remain possible when intended and supported. Added three read-only regression prompts; the earlier 14-case Luna scores do not validate these new cases. Parent edits the skill and observes; playback remains with the performing Luna agent.

## Observation-led blend correction — 22 October

The user subsequently observed that the tempo guidance had overcorrected into frequent large-tempo selections connected by simple near-file-end fades, with repeats during a provider failure. This is supported by the performer/observer ledger: repeated1–4s natural-speed ending exchanges, often across large tempo differences, even when close-tempo candidates existed. Preserving the natural sound was achieved in later snapshots; this did not establish beatmatching, phrase placement or a convincing blend.

Re-read Morse part0024 (single-phrase beatmix and bassline swap), Broughton/Brewster ch09 (overlap and EQ roles) and ch13 (placement). They distinguish deliberate end-to-end/genre changes from audible matched overlap, and explain how an earlier incoming approach can meet the outgoing main statement without waiting for file silence. The skill now first seeks a suitable unplayed close-tempo successor for a flowing set, retains modest correction and Sync, asks for an actual composite arrangement and develops range over several records. Resets remain available for a musical reason; no fixed blend quota or genre sequence is imposed. Set-local audible history, prior fresh-set exclusions and declared emergency repeats are explicit.

App defaults also contradicted the intended ordinary blending policy: both mix.options and the successor queue defaulted max_stretch_percent to100. Changed these defaults to4 with explicit wider values still supported and planner version advanced. The source-relative/local-tempo control distinction is documented: natural playback uses percent0; setting the global analyzed BPM with an anomalous local fit can itself request a substantial stretch. Master promotion preserving rate is intentional behavior, not a proven engine defect. Inspection exposes raw beat_index and anchor-relative beat_position; at_event uses the former.

The screenshot's playback-key error occurred after native sign-in, but broad CLI substring matching labeled it auth_required. Corrected that classification and added a60s native source cooldown retaining the grant, shared by normal deck preparation and batch analysis. Bulk source search stops on key refusal rather than repeatedly probing more tracks. An actual subsequent Big Jet Plane load succeeded without OAuth; this supports intermittent refusal, not proof of an account sign-out. Upstream reports suggest AES-request throttling as one possible cause: https://github.com/librespot-org/librespot/issues/1649 . No restriction bypass, alternate client identity, credential copying or extra scope was introduced.

The user stopped the performer before the planned one-hour endpoint to request fixes. The run must not be reported as a completed clean hour. New written regression cases are definitions; the earlier14-case Luna scores do not validate these changes. Do not restart a performer as part of installing the fixes without a new performance request.
