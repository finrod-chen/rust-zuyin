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
      5. 對 <PimeRoot>\x86\PIMETextService.dll／x64\PIMETextService.dll
         （存在哪個就註冊哪個）重新執行 regsvr32，讓它的 DllRegisterServer
         重新掃描所有 backend 底下的 input_methods\*\ime.json、重新註冊
         TSF 語言設定檔（見下方「已知結果」，這一步是必要的，只複製檔案、
         重啟 PIMELauncher.exe 不會讓新輸入法出現在 Windows 的語言清單）。
      6. 重啟 PIMELauncher.exe。

    已知結果（第一次實測，見 GitHub PR／issue 討論）：只做步驟 1-4、6
    （不含步驟 5 的 regsvr32 重新註冊）時，`backends.json` 有正確更新、
    PIMELauncher.exe 的除錯主控台（`PIMELauncher.exe /console`）也顯示
    正常運作，但「Rust 注音輸入法」完全沒有出現在 Windows 的語言清單、
    也從來沒有收到任何 `init` 請求——這是因為 TSF 語言設定檔的註冊是在
    `PIMETextService.dll` 的 `DllRegisterServer`（`regsvr32` 觸發）裡做的
    一次性掃描，PIMELauncher.exe 重啟並不會觸發它。加上步驟 5 後這個問題
    應該會解決，但**還沒有實際重新測試過**，麻煩照 README 的方式回報結果。

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

function Register-PimeTextService {
    # PIME 安裝時把 PIMETextService.dll 註冊成一個 COM Text Service；它的
    # DllRegisterServer（PIMETextService/DllEntry.cpp）會掃描每個 backend
    # 底下的 input_methods\*\ime.json、把每個 ime.json 的 guid 都註冊成
    # 一個 TSF 語言設定檔。這個掃描只在 regsvr32 執行時（也就是
    # DllRegisterServer 被呼叫時）發生一次，PIMELauncher.exe 重啟並不會
    # 重新觸發——所以裝好新的 backend 之後，要重新對已經註冊過的
    # PIMETextService.dll 執行一次 regsvr32，讓它重新掃描、把新加的
    # ime.json 也註冊進去，Windows 才找得到這個新輸入法。
    #
    # 32 位元與 64 位元的 DLL 要分別用對應位元的 regsvr32.exe 註冊
    # （64 位元 Windows 上，32 位元版的 regsvr32.exe 在 SysWOW64 底下，
    # 不是 System32——這兩個資料夾名稱刻意互換，是 Windows 由來已久的
    # 特例，見微軟文件）；兩個 DLL 只要存在就都重新註冊一次，因為兩種
    # 位元的應用程式（例如 32 位元的舊軟體 vs. 64 位元的瀏覽器）各自吃
    # 各自位元的 TSF text service。
    param([string]$PimeRoot)

    $system32 = Join-Path $Env:WINDIR "System32\regsvr32.exe"
    $sysWow64 = Join-Path $Env:WINDIR "SysWOW64\regsvr32.exe"
    $x86Regsvr32 = if (Test-Path -LiteralPath $sysWow64) { $sysWow64 } else { $system32 }

    $variants = @(
        @{ Label = "64 位元"; Dll = (Join-Path $PimeRoot "x64\PIMETextService.dll"); Regsvr32 = $system32 },
        @{ Label = "32 位元"; Dll = (Join-Path $PimeRoot "x86\PIMETextService.dll"); Regsvr32 = $x86Regsvr32 },
        @{ Label = "ARM64"; Dll = (Join-Path $PimeRoot "arm64\PIMETextService.dll"); Regsvr32 = $system32 }
    )

    $registeredAny = $false
    foreach ($variant in $variants) {
        if (-not (Test-Path -LiteralPath $variant.Dll)) {
            continue
        }
        Write-Host "[INFO] 重新註冊 $($variant.Label) PIMETextService.dll（讓它重新掃描 ime.json）..."
        try {
            if (-not (Test-Path -LiteralPath $variant.Regsvr32)) {
                Write-Host "[WARN] 找不到 $($variant.Regsvr32)，跳過這個位元版本。"
                continue
            }
            $argumentString = '/s "{0}"' -f $variant.Dll
            $proc = Start-Process -FilePath $variant.Regsvr32 -ArgumentList $argumentString -Wait -PassThru -WindowStyle Hidden
            if ($proc.ExitCode -eq 0) {
                Write-Host "[INFO] 註冊完成（結束碼 0）。"
                $registeredAny = $true
            }
            else {
                Write-Host "[WARN] regsvr32 回傳結束碼 $($proc.ExitCode)，可能沒有註冊成功。"
            }
        }
        catch {
            # 這一步失敗不該讓整支安裝腳本中止——寧可 PIMELauncher 照樣
            # 重啟、讓使用者看到清楚的警告，也不要卡在這裡拋出例外。
            Write-Host "[WARN] 重新註冊 $($variant.Label) 版本時發生錯誤：$($_.Exception.Message)"
        }
    }

    if (-not $registeredAny) {
        Write-Host "[WARN] 在 $PimeRoot 底下找不到任何 PIMETextService.dll（x86／x64／arm64），跳過重新註冊這一步。"
    }
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

    Register-PimeTextService -PimeRoot $resolvedPimeRoot

    Write-Host "[INFO] 安裝完成。"
}
finally {
    Start-PIMELauncher -Path $launcherPath
}

Write-Host ""
Write-Host (
    "[INFO] 接下來請打開「設定 > 時間與語言 > 語言與地區」，確認" +
    "輸入法清單裡有沒有出現「Rust 注音輸入法」；沒有的話試著登出再登入，" +
    "或重新啟動一次電腦讓 Windows 重新偵測輸入法。第一次實測發現只複製" +
    "檔案、重啟 PIMELauncher.exe 不會讓新輸入法出現，已經加上重新執行" +
    "regsvr32 這一步（見這支腳本開頭的說明），但這個修法本身還沒有實際" +
    "測過，如果這一步還是沒有出現，請照實回報看到的狀況（錯誤訊息、" +
    "上面 regsvr32 的結束碼），我們再一起排查。"
)
