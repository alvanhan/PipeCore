import json
import subprocess

process = subprocess.run(
    ["./pipecore", "import", "--schema", "import.json", "--input", "data.xlsx"],
    capture_output=True, text=True, check=False,
)
result = json.loads(process.stdout)
if process.returncode:
    raise RuntimeError(result)
print(result)
