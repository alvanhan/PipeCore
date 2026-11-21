<?php

// Copy the contents below the opening PHP tag into routes/console.php.

\Illuminate\Support\Facades\Artisan::command('pipecore:import-invoices {--validate} {--commit} {--mssql}', function () {
    if ($this->option('validate') && $this->option('commit')) {
        $this->error('Choose either --validate or --commit.');
        return 2;
    }

    $validate = $this->option('validate');
    $commit = $this->option('commit');
    $mssql = $this->option('mssql');
    $binary = base_path(PHP_OS_FAMILY === 'Windows' ? 'bin/pipecore.exe' : 'bin/pipecore');
    $workingDirectory = storage_path('app/private');

    $args = [
        $binary,
        $validate ? 'validate' : 'import',
        '--schema', config_path('pipecore/invoices-import.json'),
        '--input', $workingDirectory . DIRECTORY_SEPARATOR . 'invoices.xlsx',
    ];

    $environment = [];
    if (!$validate) {
        $name = $mssql ? 'PIPECORE_MSSQL_DSN' : 'PIPECORE_PG_DSN';
        $dsn = config($mssql ? 'pipecore.mssql_dsn' : 'pipecore.pg_dsn');
        if (!is_string($dsn) || $dsn === '') {
            $this->error("Missing $name in Laravel configuration.");
            return 2;
        }
        $environment[$name] = $dsn;

        if (!$commit) {
            $args[] = '--dry-run';
        }
    }

    $process = new \Symfony\Component\Process\Process($args, $workingDirectory, $environment);
    $process->setTimeout(900);

    try {
        $exit = $process->run();
        $result = json_decode($process->getOutput(), true, 512, JSON_THROW_ON_ERROR);
        if (!is_array($result)) {
            throw new UnexpectedValueException('Invalid result.');
        }
    } catch (RuntimeException | JsonException $error) {
        $this->error('PipeCore did not provide a result. Check the binary, paths, and process timeout.');
        if ($commit) {
            $this->error('Check the database before retrying this write.');
        }
        return 1;
    }

    $this->line(json_encode($result, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_INVALID_UTF8_SUBSTITUTE));
    if (($result['commit_outcome'] ?? null) === 'unknown') {
        $this->error('Commit outcome unknown. Check the database before retrying.');
    }
    if ($commit && $exit === 0 && ($result['committed'] ?? false) !== true) {
        $this->error('The write was not confirmed.');
        return 1;
    }

    return $exit;
});
