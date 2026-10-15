# Optional compatibility bridge. Only reads the project's two saved credentials.
# Never logs passwords or writes plaintext credentials to disk.
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
try {
    $settings = New-Object System.Xml.XmlReaderSettings
    $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
    $settings.XmlResolver = $null
    $reader = [System.Xml.XmlReader]::Create($env:SC_REDGATE_PROJECT, $settings)
    $document = New-Object System.Xml.XmlDocument
    $document.XmlResolver = $null
    try { $document.Load($reader) } finally { $reader.Dispose() }
    $assembly = [Reflection.Assembly]::LoadFrom('C:\Program Files (x86)\Red Gate\SQL Compare 14\RedGate.Legacy.Serialization.dll')
    $method = $assembly.GetType('RedGate.Legacy.Serialization.Encryption').GetMethod('DecryptString', [Reflection.BindingFlags]'Public,NonPublic,Static')
    $reply = @{}
    foreach ($side in 1,2) {
        $node = $document.SelectSingleNode("/Project/DataSource$side/Password")
        $value = ''
        if ($node -and $node.InnerText) {
            if ($node.GetAttribute('encrypted') -eq '1') { $value = $method.Invoke($null, [object[]]@($node.InnerText)) }
            else { $value = $node.InnerText }
        }
        $reply["password$side"] = $value
    }
    $reply | ConvertTo-Json -Compress
} catch {
    [Console]::Error.WriteLine('Saved Redgate credentials could not be read on this installation. Enter SQL passwords manually.')
    exit 1
}
