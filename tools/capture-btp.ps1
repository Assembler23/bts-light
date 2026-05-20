<#
.SYNOPSIS
  Zeichnet einen rohen Wire-Mitschnitt aus BTP (Tournament Planner) auf.

.DESCRIPTION
  Dieses Skript dient der Entwicklung von BTS Light. Es verbindet sich mit der
  "TP Network"-Schnittstelle von BTP, sendet eine LOGIN- und eine
  SENDTOURNAMENTINFO-Anfrage und speichert die unveraenderten Antwort-Bytes in
  zwei Dateien (btp-login.bin, btp-tournament.bin). Diese Dateien dienen als
  echte Testdaten fuer den Protokoll-Parser.

  Es wird nichts installiert und nichts an BTP veraendert - nur gelesen.

  Voraussetzung in BTP:
    Extras -> Optionen -> Veroeffentlichen -> "TP Network" aktivieren.
  BTP und dieses Skript muessen auf demselben PC oder im selben Netzwerk laufen,
  und in BTP muss ein Turnier geoeffnet sein.

.PARAMETER BtpHost
  IP-Adresse von BTP. Standard: 127.0.0.1 (derselbe PC).

.PARAMETER Port
  TCP-Port. Standard: 9901 (BTP). Fuer den Liga-Planer (BLP): 9911.

.PARAMETER Password
  Das in BTP gesetzte TP-Network-Passwort. Leer lassen, wenn keines gesetzt ist.

.EXAMPLE
  .\capture-btp.ps1

.EXAMPLE
  .\capture-btp.ps1 -Password "geheim"

.NOTES
  Start ueber: powershell -ExecutionPolicy Bypass -File .\capture-btp.ps1
#>
param(
    [string]$BtpHost = "127.0.0.1",
    [int]$Port = 9901,
    [string]$Password = ""
)

$ErrorActionPreference = "Stop"

# Baut ein VISUALXML-Request-Dokument fuer die angegebene Action.
function New-VisualXml([string]$action, [string]$password) {
    $pwItem = ""
    if ($password -ne "") {
        $esc = [System.Security.SecurityElement]::Escape($password)
        $pwItem = "<ITEM ID=""Password"" TYPE=""String"">$esc</ITEM>"
    }
    return '<?xml version="1.0" encoding="UTF-8"?><VISUALXML VERSION="1.0">' +
        '<GROUP ID="Header"><GROUP ID="Version">' +
        '<ITEM ID="Hi" TYPE="Integer">1</ITEM><ITEM ID="Lo" TYPE="Integer">1</ITEM>' +
        '</GROUP></GROUP>' +
        '<GROUP ID="Action"><ITEM ID="ID" TYPE="String">' + $action + '</ITEM>' +
        $pwItem + '</GROUP>' +
        '<GROUP ID="Client"><ITEM ID="IP" TYPE="String">bts-light</ITEM></GROUP>' +
        '</VISUALXML>'
}

# Komprimiert Bytes mit gzip.
function Compress-Gzip([byte[]]$data) {
    $ms = New-Object System.IO.MemoryStream
    # leaveOpen = $true, damit $ms nach dem GZipStream weiter nutzbar bleibt.
    $gz = New-Object System.IO.Compression.GZipStream(
        $ms, [System.IO.Compression.CompressionMode]::Compress, $true)
    $gz.Write($data, 0, $data.Length)
    $gz.Dispose()
    $result = $ms.ToArray()
    $ms.Dispose()
    return $result
}

# Sendet eine Action an BTP und speichert die rohe Antwort.
function Invoke-BtpRequest([string]$action, [string]$outFile) {
    $xml = New-VisualXml $action $Password
    $payload = Compress-Gzip ([System.Text.Encoding]::UTF8.GetBytes($xml))

    # 4-Byte-Laengenheader, Big-Endian (BitConverter liefert Little-Endian).
    $header = [System.BitConverter]::GetBytes([int]$payload.Length)
    [Array]::Reverse($header)

    $client = New-Object System.Net.Sockets.TcpClient
    $client.Connect($BtpHost, $Port)
    $stream = $client.GetStream()
    $stream.ReadTimeout = 10000

    $stream.Write($header, 0, 4)
    $stream.Write($payload, 0, $payload.Length)
    $stream.Flush()

    $response = New-Object System.IO.MemoryStream
    $buffer = New-Object byte[] 8192
    try {
        while ($true) {
            $n = $stream.Read($buffer, 0, $buffer.Length)
            if ($n -le 0) { break }
            $response.Write($buffer, 0, $n)
        }
    }
    catch [System.IO.IOException] {
        # Read-Timeout: BTP sendet nichts mehr, hat aber die Verbindung nicht
        # aktiv geschlossen. Wir nehmen die bereits empfangenen Bytes.
    }
    $client.Close()

    $bytes = $response.ToArray()
    $path = Join-Path (Get-Location) $outFile
    [System.IO.File]::WriteAllBytes($path, $bytes)
    Write-Host ("  {0,-20} -> {1} ({2} Bytes)" -f $action, $outFile, $bytes.Length) `
        -ForegroundColor Green
    return $bytes.Length
}

Write-Host ""
Write-Host "BTS Light - BTP-Mitschnitt" -ForegroundColor Cyan
Write-Host ("Verbinde mit {0}:{1} ..." -f $BtpHost, $Port)
Write-Host ""

try {
    [void](Invoke-BtpRequest "LOGIN" "btp-login.bin")
    $infoLen = Invoke-BtpRequest "SENDTOURNAMENTINFO" "btp-tournament.bin"
    Write-Host ""
    if ($infoLen -lt 50) {
        Write-Host "Hinweis: Die Antwort ist sehr klein." -ForegroundColor Yellow
        Write-Host "Vermutlich ist kein Turnier geoeffnet oder das Passwort stimmt nicht." `
            -ForegroundColor Yellow
    }
    else {
        Write-Host "Fertig. Bitte beide .bin-Dateien an die Entwickler schicken." `
            -ForegroundColor Green
    }
}
catch {
    Write-Host ""
    Write-Host ("Fehler: {0}" -f $_.Exception.Message) -ForegroundColor Red
    Write-Host ""
    Write-Host "Bitte pruefen:" -ForegroundColor Yellow
    Write-Host " - Laeuft BTP mit einem geoeffneten Turnier?"
    Write-Host " - Ist 'TP Network' aktiviert (Extras > Optionen > Veroeffentlichen)?"
    Write-Host (" - Stimmen Host ({0}) und Port ({1})?" -f $BtpHost, $Port)
}
Write-Host ""
