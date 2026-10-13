#!/usr/bin/env python3
"""Guard new native and MIDI actions against disappearing from CLI discovery."""
import json
import pathlib
import re

root = pathlib.Path(__file__).resolve().parents[1]
manifest = json.loads((root / "tools/control-coverage.json").read_text())
catalog = json.loads((root / "crates/control-protocol/src/catalog.json").read_text())
names = {entry["command"] for entry in catalog}
assert len(names) == len(catalog), "Duplicate catalog commands"
main = (root / "src-tauri/src/main.rs").read_text()
registered = main.split(".invoke_handler(tauri::generate_handler![", 1)[1].split("])", 1)[0]
handlers = {name.strip() for name in registered.split(",") if name.strip()}
mapped = set(manifest["handlers"]) | set(manifest["transport_handlers"])
assert handlers == mapped, f"Handler coverage drift: missing={handlers - mapped}, obsolete={mapped - handlers}"
assert set(manifest["handlers"].values()) <= names, "A mapped method is not callable in the catalog"
control = (root / "src-tauri/src/control.rs").read_text()
dispatch = control.split("let result = match request.command.as_str() {", 1)[1].split("\nfn resolve_source", 1)[0]
implemented = set()
for line in dispatch.splitlines():
    if re.match(r'^        "', line):
        implemented.update(re.findall(r'"([a-z_]+(?:\.[a-z_]+)*)"', line.split("=>", 1)[0]))
assert names == implemented, f"Catalog/dispatcher drift: missing={implemented - names}, obsolete={names - implemented}"
model = (root / "crates/deck-engine/src/model.rs").read_text().split("pub enum EngineCommand {", 1)[1].split("\n}", 1)[0]
variants = re.findall(r"^    ([A-Z]\w*)\s*(?:\{|,)", model, re.M)
serde_names = {re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower() for name in variants}
examples = json.loads((root / "src-tauri/src/engine-catalog.json").read_text())
assert {entry["type"] for entry in examples} == serde_names, "Engine enum/example coverage drift"
assert len(examples) == len(variants), "Duplicate engine examples"
midi = (root / "crates/flx4-midi/src/lib.rs").read_text().split("pub enum ControllerAction {", 1)[1].split("\n}", 1)[0]
midi_variants = re.findall(r"^    ([A-Z]\w*)\s*(?:\{|,)", midi, re.M)
assert set(midi_variants) == set(manifest["midi_actions"]), "MIDI action coverage drift"
print(json.dumps({"product_commands": len(names), "native_handlers": len(manifest["handlers"]), "engine_actions": len(variants), "midi_action_kinds": len(midi_variants), "coverage": "complete"}))
