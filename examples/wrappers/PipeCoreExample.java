import java.nio.charset.StandardCharsets;

public class PipeCoreExample {
    public static void main(String[] args) throws Exception {
        ProcessBuilder builder = new ProcessBuilder(
            "./pipecore", "import", "--schema", "import.json", "--input", "data.xlsx");
        builder.redirectError(ProcessBuilder.Redirect.INHERIT);
        Process process = builder.start();
        String json = new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
        int exit = process.waitFor();
        System.out.println(json); // Parse with the application's JSON library.
        if (exit != 0) throw new IllegalStateException("PipeCore failed with exit " + exit);
    }
}
