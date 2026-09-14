<#
.SYNOPSIS
    把 zuyin-backend 註冊成 PIME 的一個 backend，讓「Rust 注音輸入法」出現在
    Windows 的輸入法清單裡。

.DESCRIPTION
    這支腳本假設 PIME（https://github.com/EasyIME/PIME）本身已經另外裝好
    ——本專案不重新包裝、也不重新散布 PIME，只是把自己註冊成它認得的一個
    backend（做法對照 PIME 官方 go-backend 範例的 deploy-server.ps1，見
    docs/PIME_PROTOCOL.md「安裝／註冊」一節）。實際流程：

      1. 找到 PIME 安裝路徑（可用 -PimeRoot 指定；沒指定就找常見安裝位置）。
      2. 把 zuyin-backend.exe、詞庫（data\）複製到
         <PimeRoot>\rust-zuyin\ 底下。
      3. 把 pime-config\input_methods\zuyin\ime.json 複製到
         <PimeRoot>\rust-zuyin\input_methods\zuyin\ime.json。
      4. 把 pime-config\backends.json 裡的 "rust-zuyin" 項目合併進
         <PimeRoot>\backends.json（保留其他既有 backend，不覆蓋整個檔案；
         重複執行這支腳本是安全的，會直接更新同一個項目）。
      5. 重啟 PIMELauncher.exe，讓它重新掃描 input_methods\*\ime.json。

    這支腳本沒有實際在 Windows 上跑過（開發環境沒有 Windows 機器），如果
    跑起來哪裡不對，請照著錯誤訊息回報，不要照抄別的地方的做法硬修。

.PARAMETER PimeRoot
    PIME 安裝路徑。預設會依序嘗試
    "$Env:ProgramFiles(x86)\PIME"、"$Env:ProgramFiles\PIME"；
    都找不到 PIMELauncher.exe 就要求手動指定。

.PARAMETER SourceDir
    要安裝的檔案來源目錄，預期底下有 zuyin-backend.exe（或
    target\release\zuyin-backend.exe）、data\chewing-characters.txt、
    pime-config\。預設是這支腳本所在目錄的上一層（不管是直接在原始碼
    repo 裡執行、還是在解壓縮的 release zip 裡執行，這支腳本都跟著放在
    同一層結構的 scripts\ 底下，見 README.md「下載執行檔」）。

.EXAMPLE
    # 在 repo 根目錄先建置好，再執行（需要系統管理員權限，腳本會自動要求）：
    cargo build --release -p zuyin-backend
    .\scripts\install-windows.ps1

.EXAMPLE
    # 指定 PIME 安裝在非預設路徑：
    .\scripts\install-windows.ps1 -PimeRoot "D:\Tools\PIME"
#>
[CmdletBinding()]
param(
    [string]$PimeRoot,
    [string]$SourceDir = (Join-Path $PSScriptRoot "..")
)

$ErrorActionPreference = "Stop"

$BackendName = "rust-zuyin"

function Test-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Restart-Elevated {
    # 複製 PIME 安裝路徑（通常在 Program Files 底下）需要系統管理員權限；
    # 沒有的話用同樣的參數重新以提升權限開一個新的 PowerShell 視窗執行。
    $argumentList = [System.Collections.Generic.List[string]]::new()
    $argumentList.Add("-NoProfile")
    $argumentList.Add("-ExecutionPolicy")
    $argumentList.Add("Bypass")
    $argumentList.Add("-File")
    $argumentList.Add('"{0}"' -f $PSCommandPath)
    if ($PimeRoot) {
        $argumentList.Add("-PimeRoot")
        $argumentList.Add('"{0}"' -f $PimeRoot)
    }
    $argumentList.Add("-SourceDir")
    $argumentList.Add('"{0}"' -f $SourceDir)

    Write-Host "[INFO] 需要系統管理員權限，重新以提升權限執行 ..."
    try {
        Start-Process -FilePath "powershell.exe" -Verb RunAs -ArgumentList $argumentList.ToArray() | Out-Null
        Write-Host "[INFO] 已在新視窗以系統管理員權限執行，這個視窗可以關閉了。"
        exit 0
    }
    catch {
        throw "取消或無法取得系統管理員權限。"
    }
}

function Find-PimeRoot {
    param([string]$Explicit)

    if ($Explicit) {
        if (Test-Path -LiteralPath (Join-Path $Explicit "PIMELauncher.exe")) {
            return $Explicit
        }
        throw "指定的 -PimeRoot 底下找不到 PIMELauncher.exe：$Explicit"
    }

    # ProgramFiles(x86) 只在 64 位元 Windows 上存在；先濾掉不存在的環境
    # 變數再組路徑，避免 Join-Path 收到 $null 直接丟例外。
    $bases = @($Env:ProgramFiles, ${Env:ProgramFiles(x86)}) | Where-Object { $_ }
    $candidates = $bases | ForEach-Object { Join-Path $_ "PIME" }
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath (Join-Path $candidate "PIMELauncher.exe")) {
            return $candidate
        }
    }

    $tried = if ($candidates) { $candidates -join '、' } else { '（找不到任何 ProgramFiles 環境變數）' }
    throw (
        "找不到 PIME 安裝路徑（試過：$tried）。" +
        "請先從 https://github.com/EasyIME/PIME 安裝 PIME（本專案不重新" +
        "包裝、也不重新散布），或用 -PimeRoot 指定實際安裝路徑。"
    )
}

function Find-BackendExe {
    param([string]$Source)

    $candidates = @(
        (Join-Path $Source "zuyin-backend.exe"),
        (Join-Path $Source "target\release\zuyin-backend.exe")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    throw (
        "找不到 zuyin-backend.exe（試過：$($candidates -join '、')）。" +
        "請先執行 cargo build --release -p zuyin-backend，" +
        "或改用 GitHub Releases 上打包好的 zuyin-backend-windows-x64.zip。"
    )
}

function Stop-PIMELauncher {
    param([string]$Path)

    $running = @(Get-Process -Name "PIMELauncher" -ErrorAction SilentlyContinue)
    if (-not $running) {
        Write-Host "[INFO] PIMELauncher.exe 目前沒有在執行。"
        return
    }

    Write-Host "[INFO] 停止 PIMELauncher.exe ..."
    if (Test-Path -LiteralPath $Path) {
        try {
            Start-Process -FilePath $Path -ArgumentList "/quit" -WindowStyle Hidden | Out-Null
        }
        catch {
            Write-Host "[WARN] 正常關閉失敗，稍後將強制結束程序。"
        }
    }

    $deadline = (Get-Date).AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 250
        $running = @(Get-Process -Name "PIMELauncher" -ErrorAction SilentlyContinue)
    } while ($running.Count -gt 0 -and (Get-Date) -lt $deadline)

    if ($running.Count -gt 0) {
        Write-Host "[WARN] 等待正常關閉逾時，強制結束程序。"
        $running | Stop-Process -Force
    }

    Start-Sleep -Seconds 1
    Write-Host "[INFO] PIMELauncher.exe 已停止。"
}

function Start-PIMELauncher {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "找不到 PIMELauncher.exe：$Path"
    }

    Write-Host "[INFO] 啟動 PIMELauncher.exe ..."
    Start-Process -FilePath $Path | Out-Null
    Write-Host "[INFO] PIMELauncher.exe 已啟動。"
}

function ConvertTo-JsonArrayText {
    # PowerShell 5.1（Windows 內建版本）的 ConvertTo-Json 在陣列剛好只有
    # 一個元素時，會自動「拆箱」成單一物件而不是陣列，即使輸入明明是
    # 陣列——這裡逐一序列化每個元素、自己組出陣列的中括號跟逗號，確保
    # 不管有幾個元素，輸出永遠是合法的 JSON 陣列。
    param([object[]]$Items, [int]$Depth = 5)

    if (-not $Items -or $Items.Count -eq 0) {
        return "[]"
    }
    $parts = foreach ($item in $Items) { $item | ConvertTo-Json -Depth $Depth }
    return "[`n" + ($parts -join ",`n") + "`n]"
}

function Merge-BackendEntry {
    param(
        [string]$BackendsJsonPath,
        [PSCustomObject]$NewEntry
    )

    $entries = @()
    if (Test-Path -LiteralPath $BackendsJsonPath) {
        $raw = Get-Content -LiteralPath $BackendsJsonPath -Raw -Encoding UTF8
        if ($raw -and $raw.Trim()) {
            # 單一元素的陣列在 PS5.1 會被 ConvertFrom-Json 解成單一物件，
            # 用 @() 強制轉回陣列（跟上面 ConvertTo-Json 的坑是同一類）。
            $entries = @($raw | ConvertFrom-Json)
        }
    }
    else {
        Write-Host "[WARN] $BackendsJsonPath 不存在，將建立一份只有本專案的新檔案。"
    }

    $existingIndex = -1
    for ($i = 0; $i -lt $entries.Count; $i++) {
        if ($entries[$i].name -eq $NewEntry.name) {
            $existingIndex = $i
            break
        }
    }

    if ($existingIndex -ge 0) {
        Write-Host "[INFO] backends.json 已有「$($NewEntry.name)」項目，更新它。"
        $entries[$existingIndex] = $NewEntry
    }
    else {
        Write-Host "[INFO] backends.json 新增「$($NewEntry.name)」項目。"
        $entries += $NewEntry
    }

    $json = ConvertTo-JsonArrayText -Items $entries
    Set-Content -LiteralPath $BackendsJsonPath -Value $json -Encoding UTF8
}

# ---- 主流程 ----

if (-not (Test-Admin)) {
    Restart-Elevated
}

$resolvedPimeRoot = Find-PimeRoot -Explicit $PimeRoot
Write-Host "[INFO] PIME 安裝路徑：$resolvedPimeRoot"

$backendExe = Find-BackendExe -Source $SourceDir
Write-Host "[INFO] 使用的 zuyin-backend.exe：$backendExe"

$dictionaryPath = Join-Path $SourceDir "data\chewing-characters.txt"
if (-not (Test-Path -LiteralPath $dictionaryPath)) {
    throw "找不到詞庫檔案：$dictionaryPath"
}

$imeJsonSource = Join-Path $SourceDir "pime-config\input_methods\zuyin\ime.json"
$backendsJsonSnippet = Join-Path $SourceDir "pime-config\backends.json"
foreach ($required in @($imeJsonSource, $backendsJsonSnippet)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "找不到必要的設定檔：$required（這支腳本要跟 pime-config\ 放在同一層）"
    }
}

$launcherPath = Join-Path $resolvedPimeRoot "PIMELauncher.exe"
$installDir = Join-Path $resolvedPimeRoot $BackendName
$inputMethodDir = Join-Path $installDir "input_methods\zuyin"
$destBackendsJson = Join-Path $resolvedPimeRoot "backends.json"

try {
    Stop-PIMELauncher -Path $launcherPath

    Write-Host "[INFO] 複製檔案到 $installDir ..."
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $installDir "data") | Out-Null
    New-Item -ItemType Directory -Force -Path $inputMethodDir | Out-Null

    Copy-Item -LiteralPath $backendExe -Destination (Join-Path $installDir "zuyin-backend.exe") -Force
    Copy-Item -LiteralPath $dictionaryPath -Destination (Join-Path $installDir "data\chewing-characters.txt") -Force
    Copy-Item -LiteralPath $imeJsonSource -Destination (Join-Path $inputMethodDir "ime.json") -Force

    $userPhrasesExample = Join-Path $SourceDir "data\user_phrases.example.txt"
    if (Test-Path -LiteralPath $userPhrasesExample) {
        Copy-Item -LiteralPath $userPhrasesExample -Destination (Join-Path $installDir "data\user_phrases.example.txt") -Force
    }

    $newEntry = Get-Content -LiteralPath $backendsJsonSnippet -Raw -Encoding UTF8 | ConvertFrom-Json
    $newEntry = @($newEntry)[0]
    Merge-BackendEntry -BackendsJsonPath $destBackendsJson -NewEntry $newEntry

    Write-Host "[INFO] 安裝完成。"
}
finally {
    Start-PIMELauncher -Path $launcherPath
}

Write-Host ""
Write-Host (
    "[INFO] 接下來請打開「設定 > 時間與語言 > 語言與地區」，確認" +
    "輸入法清單裡有沒有出現「Rust 注音輸入法」；沒有的話試著登出再登入，" +
    "或重新啟動一次電腦讓 Windows 重新偵測輸入法。這支腳本沒有在真正的" +
    "Windows 環境驗證過，如果這一步沒有出現，請回報實際看到的狀況" +
    "（而不是照抄其他輸入法的教學硬改設定），我們再一起排查。"
)
