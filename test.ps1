param([switch]$Integration)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$env:PATH = "$PSScriptRoot\.tools\mingw64\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
& cargo test --locked
if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
if (-not $Integration) { return }
& cargo build --locked --bin schema-compare-cli
if ($LASTEXITCODE -ne 0) { throw 'CLI build failed' }
$instance = 'SchemaCompare_' + [Guid]::NewGuid().ToString('N').Substring(0,8)
$server = "(localdb)\$instance"
$resultDirectory = Join-Path $PSScriptRoot "test-results\$instance"
New-Item -ItemType Directory -Force $resultDirectory | Out-Null
& SqlLocalDB create $instance
if ($LASTEXITCODE -ne 0) { throw 'LocalDB creation failed' }
try {
    & SqlLocalDB start $instance
    if ($LASTEXITCODE -ne 0) { throw 'LocalDB start failed' }
    $fixture = (Get-Content tests\setup.sql -Raw).Replace('$(DataDir)', $resultDirectory.Replace("'","''"))
    $fixture | Set-Content "$resultDirectory\setup.sql" -Encoding utf8BOM
    & sqlcmd -S $server -E -C -b -i "$resultDirectory\setup.sql"
    if ($LASTEXITCODE -ne 0) { throw 'Fixture setup failed' }
    $cli = Join-Path $PSScriptRoot 'target\x86_64-pc-windows-gnu\debug\schema-compare-cli.exe'
    $databases = & $cli databases --server $server
    if ($LASTEXITCODE -ne 0 -or $databases -notcontains 'SC_Rust_Source' -or $databases -notcontains 'SC_Rust_Target') { throw 'Database discovery failed' }
    & $cli capture --server $server --database SC_Rust_Source --output "$resultDirectory\source.json"
    if ($LASTEXITCODE -ne 0) { throw 'Source capture failed' }
    $sourceCatalog = Get-Content "$resultDirectory\source.json" -Raw | ConvertFrom-Json
    if (@($sourceCatalog.objects | Where-Object {$_.name -eq 'ToolingFixture' -or $_.parent -eq 'ToolingFixture'}).Count -ne 0) { throw 'Diagram/tooling objects leaked into the catalog' }
    if (@($sourceCatalog.objects | Where-Object {$_.kind -eq 'Synonym' -and $_.name -eq 'CustomerAlias'}).Count -ne 1) { throw 'Synonym capture failed' }
    & $cli capture --server $server --database SC_Rust_Target --output "$resultDirectory\target.json"
    if ($LASTEXITCODE -ne 0) { throw 'Target capture failed' }
    $env:SC_TEST_SERVER=$server
    try {
        & cargo test --locked --test deployment_live -- --ignored
        if($LASTEXITCODE -ne 0){throw 'Deployment rollback test failed'}
    } finally { Remove-Item Env:\SC_TEST_SERVER }
    & $cli compare "$resultDirectory\source.json" "$resultDirectory\target.json" --script "$resultDirectory\deployment.sql" --report "$resultDirectory\before.json"
    if ($LASTEXITCODE -ne 0) { throw 'Script generation failed' }
    $deploymentProject = @{
        source = @{snapshot=$true;path="$resultDirectory\source.json";connection=@{server=$server;database='SC_Rust_Source';windows_auth=$true;username='';trust_certificate=$true}}
        target = @{snapshot=$false;path='';connection=@{server=$server;database='SC_Rust_Target';windows_auth=$true;username='';trust_certificate=$true}}
        options = @{}
    }
    $deploymentProject | ConvertTo-Json -Depth 5 | Set-Content "$resultDirectory\deploy.project.json" -Encoding utf8NoBOM
    # Prove the same backend as the Deploy button refuses a changed schema without executing.
    & sqlcmd -S $server -d SC_Rust_Target -E -C -b -Q 'CREATE TABLE dbo.DriftGuardFixture (Id int NOT NULL);'
    if($LASTEXITCODE -ne 0){throw 'Drift fixture failed'}
    & $cli deploy "$resultDirectory\deploy.project.json" "$resultDirectory\source.json" "$resultDirectory\target.json"
    if($LASTEXITCODE -eq 0){throw 'Deploy accepted schema drift'}
    & sqlcmd -S $server -d SC_Rust_Target -E -C -b -Q "IF COL_LENGTH('dbo.Customers','Email') IS NOT NULL THROW 50000,'Drift guard executed changes',1; DROP TABLE dbo.DriftGuardFixture;"
    if($LASTEXITCODE -ne 0){throw 'Drift guard assertion failed'}
    & $cli deploy "$resultDirectory\deploy.project.json" "$resultDirectory\source.json" "$resultDirectory\target.json" --report "$resultDirectory\deployment-verification.json"
    if ($LASTEXITCODE -ne 0) { throw 'Deployment failed' }
    & $cli capture --server $server --database SC_Rust_Target --output "$resultDirectory\after.json"
    if ($LASTEXITCODE -ne 0) { throw 'Recapture failed' }
    & $cli compare "$resultDirectory\source.json" "$resultDirectory\after.json" --report "$resultDirectory\verification.json"
    if ($LASTEXITCODE -ne 0) { throw 'Verification failed' }
    $report = Get-Content "$resultDirectory\verification.json" -Raw | ConvertFrom-Json
    if (@($report.differences | Where-Object status -In 'Different','SourceOnly').Count -ne 0) { throw 'Schema did not converge' }
    & sqlcmd -S $server -d SC_Rust_Target -E -C -b -Q "IF (SELECT COUNT(*) FROM dbo.Customers WHERE Name=N'Existing customer')<>1 THROW 50000,'Data preservation failed',1; IF OBJECT_ID('dbo.LegacyLog') IS NULL THROW 50000,'Unexpected drop',1;"
    if ($LASTEXITCODE -ne 0) { throw 'Preservation assertions failed' }
    & sqlcmd -S $server -d SC_Rust_Target -E -C -b -Q "IF (SELECT COUNT(*) FROM dbo.BudgetLines WHERE (Id=1 AND PercentThreshold=123) OR (Id=2 AND PercentThreshold IS NULL) OR (Id=3 AND PercentThreshold=-2147483648) OR (Id=4 AND PercentThreshold=2147483647))<>4 THROW 50000,'ALTER COLUMN data preservation failed',1; IF (SELECT COUNT(*) FROM dbo.BudgetReferences)<>4 THROW 50000,'FK data preservation failed',1; IF NOT EXISTS(SELECT 1 FROM sys.columns WHERE object_id=OBJECT_ID('dbo.BudgetLines') AND name='PercentThreshold' AND precision=21 AND scale=6) THROW 50000,'Decimal migration failed',1; IF NOT EXISTS(SELECT 1 FROM sys.foreign_keys WHERE name='FK_BudgetReferences_Threshold' AND is_disabled=0 AND is_not_trusted=0) THROW 50000,'Foreign key was not restored',1; IF NOT EXISTS(SELECT 1 FROM sys.indexes WHERE object_id=OBJECT_ID('dbo.BudgetLines') AND name='IX_BudgetLines_Threshold') THROW 50000,'Index was not restored',1; IF NOT EXISTS(SELECT 1 FROM sys.check_constraints WHERE name='CK_BudgetLines_Threshold' AND is_disabled=0 AND is_not_trusted=0) THROW 50000,'Check constraint was not restored',1; IF NOT EXISTS(SELECT 1 FROM sys.default_constraints WHERE name='DF_BudgetLines_Threshold') THROW 50000,'Default was not restored',1;"
    if ($LASTEXITCODE -ne 0) { throw 'ALTER COLUMN preservation assertions failed' }
    & sqlcmd -S $server -d SC_Rust_Target -E -C -b -Q "IF (SELECT COUNT(*) FROM dbo.FiscalItems WHERE (Id=1 AND CreatePoint=N'Short' AND Amount=12 AND Replacement IS NULL) OR (Id=2 AND CreatePoint IS NULL AND Amount IS NULL AND Replacement IS NULL))<>2 THROW 50000,'Source column migration changed retained data',1; IF COL_LENGTH('dbo.FiscalItems','Legacy') IS NOT NULL THROW 50000,'Target-only column was not removed',1; IF COL_LENGTH('dbo.FiscalItems','CreatePoint')<>1000 THROW 50000,'nvarchar(500) was not applied',1; IF EXISTS(SELECT 1 FROM sys.indexes WHERE object_id=OBJECT_ID('dbo.FiscalItems') AND name='IX_FiscalLegacy') THROW 50000,'Removed-column index remains',1; IF NOT EXISTS(SELECT 1 FROM sys.indexes WHERE object_id=OBJECT_ID('dbo.FiscalItems') AND name='IX_FiscalReplacement') THROW 50000,'Source replacement index is missing',1;"
    if ($LASTEXITCODE -ne 0) { throw 'Source column authority assertions failed' }
    Write-Output "PASS: schema convergence and existing data/target-only object preservation. Evidence: $resultDirectory"
} finally {
    # Only the uniquely named instance created by this invocation is stopped/removed.
    & SqlLocalDB stop $instance
    & SqlLocalDB delete $instance
}
