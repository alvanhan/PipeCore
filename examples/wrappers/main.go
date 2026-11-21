package main

import (
    "bytes"
    "encoding/json"
    "fmt"
    "os"
    "os/exec"
)

func main() {
    cmd := exec.Command("./pipecore", "import", "--schema", "import.json", "--input", "data.xlsx")
    var diagnostic bytes.Buffer
    cmd.Stderr = &diagnostic
    out, err := cmd.Output()
    var result map[string]any
    if parseErr := json.Unmarshal(out, &result); parseErr != nil { fmt.Fprintln(os.Stderr, "Invalid CLI response:", parseErr); os.Exit(1) }
    fmt.Println(result)
    if err != nil { os.Exit(1) }
}
