"""Regenerate bundle icon sizes from the approved Imagegen master (requires npm dependencies)."""
from pathlib import Path
import shutil
import subprocess
import tempfile
root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="pulsedeck-icons-") as tmp:
    subprocess.run(["npm", "run", "tauri", "--", "icon", str(root / "src-tauri/icons/pulsedeck-master.png"), "--output", tmp], cwd=root, check=True)
    for name in ["32x32.png", "64x64.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.icns", "icon.ico"]:
        shutil.copy2(Path(tmp) / name, root / "src-tauri/icons" / name)
