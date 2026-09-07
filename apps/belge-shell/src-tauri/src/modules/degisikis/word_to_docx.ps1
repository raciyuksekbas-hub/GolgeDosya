param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $SourcePath,

    [Parameter(Mandatory = $true, Position = 1)]
    [string] $DestinationPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$word = $null
$document = $null
$exitCode = 0
$marker = ""

try {
    try {
        $word = New-Object -ComObject Word.Application
    }
    catch {
        $marker = "DOC_WINDOWS_WORD_REQUIRED"
        $exitCode = 20
        throw
    }

    $word.Visible = $false
    $word.DisplayAlerts = 0
    try {
        # msoAutomationSecurityForceDisable: do not execute macros while converting.
        $word.AutomationSecurity = 3
    }
    catch {
        # Older Word versions may not expose this property; conversion remains local.
    }

    try {
        # A deliberately unavailable probe prevents password prompts. It is not a credential.
        $passwordProbe = "DegisikIs-Password-Not-Supplied"
        $document = $word.Documents.Open(
            $SourcePath,
            $false,
            $true,
            $false,
            $passwordProbe,
            "",
            $false,
            "",
            "",
            0,
            $null,
            $false,
            $false,
            $false,
            0,
            $true
        )
    }
    catch {
        $message = [string] $_.Exception.Message
        if ($message -match "(?i)password|encrypted|encryption|protected|parola|şifre") {
            $marker = "DOC_PASSWORD"
            $exitCode = 21
        }
        else {
            $marker = "DOC_INVALID"
            $exitCode = 22
        }
        throw
    }

    # wdFormatDocumentDefault (16) saves DOCX without changing the source DOC.
    $document.SaveAs2($DestinationPath, 16)
}
catch {
    if ($exitCode -eq 0) {
        $marker = "DOC_CONVERSION"
        $exitCode = 23
    }
}
finally {
    if ($null -ne $document) {
        try { $document.Close(0) } catch {}
        try { [void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($document) } catch {}
    }
    if ($null -ne $word) {
        try { $word.Quit(0) } catch {}
        try { [void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) } catch {}
    }
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
}

if ($exitCode -ne 0) {
    [Console]::Error.WriteLine($marker)
}
exit $exitCode
