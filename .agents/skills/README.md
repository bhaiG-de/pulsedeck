# PulseDeck agent skills

Three skills separate saved track knowledge, musical decisions and software execution. The agent chooses the music; PulseDeck supplies observations and runs accepted controls on its audio clock.

| Skill | Responsibility |
|---|---|
| [Manage Library](pulsedeck-library/SKILL.md) (`pulsedeck-library`) | Sources and collection reads, playlists, tags, stored analysis, canonical cues/grid edits and rekordbox XML |
| [DJing — the art](djing-art/SKILL.md) (`djing-art`) | Selection, entry/exit judgment, phrase and arrangement relationships, tempo/harmony, energy, role transfers and musical evaluation |
| [Controlling PulseDeck and workflows](pulsedeck-control/SKILL.md) (`pulsedeck-control`) | Deck preparation, playback/Sync, native timed transitions, continuous sets, routing/MIDI, recording, recovery and takeover |

Each entrypoint links focused references. Read only those needed for the current request. Shared [CLI](references/cli.md), [persistent execution](references/timed-execution.md), [analysis semantics](references/musical-evidence.md) and [named anchors](references/named-cues-and-suggestions.md) describe contracts used by more than one skill. They are supporting documents, not additional skills.

## Route complete requests

| Request | Skills and outcome |
|---|---|
| Find tracks, inspect a change, mark a requested cue, or migrate XML | Library; add art for a musical recommendation |
| Plan a set or explain a transition without playback | Art plus library observations; remain read-only |
| Save the chosen set | Art chooses the sequence; library verifies exact saved order |
| Prepare B without beginning | Control prepares and leaves B paused; art chooses an entry when needed |
| Mix now or run this playlist | Art chooses the musical relationship; control performs and verifies it; library supplies scoped source knowledge |
| Record this mix | Control checks source eligibility, starts the requested capture and finalizes at its requested endpoint |
| Diagnose silence or configure headphones/MIDI | Control traces the actual signal path and applies only the requested setup/repair |
| Stop automation or hand over | Control cancels owned pending work and preserves the user's values; stop music/recording according to the request |

A performance request authorizes its necessary preparation and transitions. Planning does not authorize playback or durable metadata edits. Cue/grid changes, saved playlists, capture and account setup require that intent. Honor authorization already present without repeating permission questions.

The art skill follows the [initial research](../../research/djing-art-research.md) and the [deeper research on musical decisions](../../research/djing-art-deep-research-2026-10.md). It chooses what A should finish and what should arrive in B, traces both arrangements through the overlap, then counts backward to a feasible launch. Matching BPM, downbeats or key labels cannot establish that relationship. Generated cues and sections are hypotheses; listening and user feedback remain necessary for musical quality.

## Discovery and installation

Repository-aware clients discover the three `.agents/skills/*/SKILL.md` entrypoints. Explicit invocations are `$pulsedeck-library`, `$djing-art` and `$pulsedeck-control`. If a client caches discovery, rescan skills or start a fresh chat after replacing the old package.

Other clients can read an entrypoint by path, or install the complete package into a fresh client-managed directory:

```sh
python3 tools/install-dj-skills.py --destination /path/to/agent/skills
```

The installer stages all three skills, copies each needed shared reference into its skill, rebases links and verifies local-link closure before publishing. The three installed siblings form one package and require no checkout paths. Existing new or legacy skill folders are refused before writes; inspect and explicitly remove/relocate a previous installed package before reinstalling. Unrelated skills are preserved. Installation does not build software, change accounts, start playback or configure MCP.

Bind the intended executable/socket through the control skill's CLI reference. Use the existing running app for performance. If a build is required for setup/development:

```sh
bash tools/rust-env.sh cargo build -p pulsedeck-cli
npm run package -- --debug
```

## Validation and limits

```sh
python3 tools/validate-dj-skills.py
python3 tools/test-dj-skill-packaging.py
```

These check exactly three entrypoints, metadata, local references, shell/JSON example syntax, fresh installation, relocated-package links, collision protection and failure cleanup. They do not run audio or establish musical behavior. The skill creator's frontmatter validator can additionally check each entrypoint.

The art skill includes [14 worked cases](djing-art/references/worked-cases.md) for phrase reasoning and failure detection. These are teaching/review cases, not independent agent trials. Test unseen arrangements and audition real transitions before drawing conclusions about musical improvement.

The older [validation results](../../research/dj-skills-validation.json) concern the superseded seven skills and historical builds; their results are not validation of this rewrite. The existing `bash tools/test-dj-skills.sh` runs a separate generated-fixture integration workflow in an isolated profile with master and cue muted. It needs the built client/app and audio device and does not assess musical taste, hardware audibility or a live provider session.

The retired entrypoints were track-inspection, set-preparation, prepare-next-track, transition, session-dj, recording-and-export, and setup-and-troubleshooting. Their library, musical and operational responsibilities now belong to the three skills above; artist study notes and source references were retained under the art skill. No compatibility skill wrappers remain.

## Reference ownership

The entrypoints stay short; deeper references are grouped by the decisions they own. Art separates evidence interpretation, pulse/groove, phrase selection, audible blending, harmony, selection/energy, and complete transition design. The phrase checker checks clock coordinates only. Library separates collections, source knowledge and analysis persistence, and exchange. Control separates state/ownership, preparation, execution, ongoing sessions, setup/recovery, and recording. Cross-references connect those decisions without creating extra skills.

The most recent phrase-check trial failed the requested musical objective: the agent chose abrupt deck exchanges. Its checked cue coordinates do not count as successful blends. The revised teaching has passed packaging and arithmetic checks; a new blind live set and listening review are still required to establish improved performance.

Saved cues are manual or imported. Automatic cue placement and section classification, their settings, and vocabulary/section commands have been removed. Analysis still supplies tempo, beat/downbeat timing, key and waveform bands. The art skill must reason from available observations and human annotations; missing cues are not an analysis failure.

The DJ skills are references, not a compulsory AutoDJ workflow. Agents can choose individual moves and direct tools. Start with the [style and phrase manual](djing-art/references/style-and-phrase-manual.md) and [instructor transition studies](djing-art/references/worked-cases.md). Musical estimates are guidance; explicitly requested limits and source/ownership protections still apply.
