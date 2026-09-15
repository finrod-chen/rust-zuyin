<#
.SYNOPSIS
    完整移除已透過 install-windows.ps1 註冊進 PIME 的 zuyin-backend——包含
    Windows 語言清單裡的「Rust 注音輸入法」、backends.json 裡的項目，以及
    複製過去的執行檔與詞庫。

.DESCRIPTION
    對稱於 install-windows.ps1（見該檔案開頭的說明與 docs/PIME_PROTOCOL.md
    「安裝／註冊」一節）。實際流程：

      1. 停止 PIMELauncher.exe。
      2. 對 PIMETextService.dll（x86／x64／arm64，存在哪個就處理哪個）
         執行 "regsvr32 /u"，解除註冊 TSF 語言設定檔。
      3. 從 backends.json 移除 "rust-zuyin" 這個項目。
      4. 刪除 <PimeRoot>\rust-zuyin\ 整個資料夾（zuyin-backend.exe、詞庫、
         ime.json）。
      5. 重新對 PIMETextService.dll 執行 regsvr32（不加 /u），讓它的
         DllRegisterServer 重新掃描這時候還在的每個 backend 資料夾——
         第 4 步已經刪掉 rust-zuyin，這次掃描不會再找到它，其餘 backend
         （新酷音等）則照常恢復註冊。
      6. 重啟 PIMELauncher.exe。

    **為什麼要先解除註冊、重新掃描，而不是只刪檔案**：TSF 語言設定檔的
    註冊是 PIMETextService.dll 整個 COM text service 共用同一個 CLSID
    （EasyIME/libIME2 的 Ime::ImeModule::registerServer／
    unregisterServer，底層呼叫 ITfInputProcessorProfiles::Register／
    Unregister），沒辦法只解除單一 backend 的語言設定檔——
    unregisterServer 會把這個 CLSID 底下所有 backend（新酷音、倉頡…等）
    的語言設定檔一次全部移除，registerServer 也只會新增、不會挑著移除
    「輸入清單裡已經有、但磁碟上檔案不在了」的項目。所以只刪 rust-zuyin
    的資料夾、不重新跑這兩步 regsvr32，Windows 語言清單裡會留下一個
    指向不存在檔案的殘影項目；而如果只解除註冊、不刪資料夾就重新註冊，
    rust-zuyin 又會被原封不動地掃回來，等於沒移除。第 2～5 步之間，其他
    PIME 輸入法會短暫從 Windows 語言清單消失，這是必要的中間狀態、不是
    bug，跟直接移除、重裝整個 PIME 的行為一致，第 5 步做完就會恢復。

.PARAMETER PimeRoot
    PIME 安裝路徑。規則與 install-windows.ps1 相同：預設依序嘗試
    "$Env:ProgramFiles(x86)\PIME"、"$Env:ProgramFiles\PIME"；都找不到
    PIMELauncher.exe 就要求手動指定。

.PARAMETER RemoveUserData
    另外刪除 "%APPDATA%\rust-zuyin\"（使用者自訂詞庫）。預設不會刪除
    ——那是使用者自己輸入、記憶的自訂詞，移除輸入法本身不代表要連這份
    資料都丟掉；確定要一併清掉才加這個參數。

.EXAMPLE
    .\scripts\uninstall-windows.ps1

.EXAMPLE
    # 連同使用者自訂詞庫一起清掉：
    .\scripts\uninstall-windows.ps1 -RemoveUserData

.EXAMPLE
    # PIME 裝在非預設路徑：
    .\scripts\uninstall-windows.ps1 -PimeRoot "D:\Tools\PIME"
#>
[CmdletBinding()]
param(
    [string]$PimeRoot,
    [switch]$RemoveUserData
)

$ErrorActionPreference = "Stop"

$BackendName = "rust-zuyin"

function Test-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Restart-Elevated {
    # 解除／重新註冊 PIMETextService.dll、刪除 Program Files 底下的檔案
    # 都需要系統管理員權限；沒有的話用同樣的參數重新以提升權限開一個新
    # 的 PowerShell 視窗執行。
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
    if ($RemoveUserData) {
        $argumentList.Add("-RemoveUserData")
    }

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
        "如果 PIME 本身已經移除，這支腳本也沒有東西好清了；" +
        "如果只是裝在非預設路徑，請用 -PimeRoot 指定實際安裝路徑。"
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
        Write-Host "[WARN] 找不到 PIMELauncher.exe（$Path），略過重新啟動。"
        return
    }

    Write-Host "[INFO] 啟動 PIMELauncher.exe ..."
    Start-Process -FilePath $Path | Out-Null
    Write-Host "[INFO] PIMELauncher.exe 已啟動。"
}

function Invoke-PimeTextServiceRegistration {
    # 32 位元與 64 位元的 DLL 要分別用對應位元的 regsvr32.exe 處理
    # （64 位元 Windows 上，32 位元版的 regsvr32.exe 在 SysWOW64 底下，
    # 不是 System32——這兩個資料夾名稱刻意互換，是 Windows 由來已久的
    # 特例，見微軟文件）；兩個 DLL 只要存在就都處理一次。
    param(
        [string]$PimeRoot,
        [switch]$Unregister
    )

    $system32 = Join-Path $Env:WINDIR "System32\regsvr32.exe"
    $sysWow64 = Join-Path $Env:WINDIR "SysWOW64\regsvr32.exe"
    $x86Regsvr32 = if (Test-Path -LiteralPath $sysWow64) { $sysWow64 } else { $system32 }

    $variants = @(
        @{ Label = "64 位元"; Dll = (Join-Path $PimeRoot "x64\PIMETextService.dll"); Regsvr32 = $system32 },
        @{ Label = "32 位元"; Dll = (Join-Path $PimeRoot "x86\PIMETextService.dll"); Regsvr32 = $x86Regsvr32 },
        @{ Label = "ARM64"; Dll = (Join-Path $PimeRoot "arm64\PIMETextService.dll"); Regsvr32 = $system32 }
    )

    $verb = if ($Unregister) { "解除註冊" } else { "重新註冊" }
    $processedAny = $false
    foreach ($variant in $variants) {
        if (-not (Test-Path -LiteralPath $variant.Dll)) {
            continue
        }
        Write-Host "[INFO] $verb $($variant.Label) PIMETextService.dll ..."
        try {
            if (-not (Test-Path -LiteralPath $variant.Regsvr32)) {
                Write-Host "[WARN] 找不到 $($variant.Regsvr32)，跳過這個位元版本。"
                continue
            }
            $argumentString = if ($Unregister) {
                '/u /s "{0}"' -f $variant.Dll
            }
            else {
                '/s "{0}"' -f $variant.Dll
            }
            $proc = Start-Process -FilePath $variant.Regsvr32 -ArgumentList $argumentString -Wait -PassThru -WindowStyle Hidden
            if ($proc.ExitCode -eq 0) {
                Write-Host "[INFO] 完成（結束碼 0）。"
                $processedAny = $true
            }
            else {
                Write-Host "[WARN] regsvr32 回傳結束碼 $($proc.ExitCode)，可能沒有成功。"
            }
        }
        catch {
            # 這一步失敗不該讓整支腳本中止——寧可繼續完成後續步驟、讓
            # PIMELauncher 照樣重啟，也不要卡在這裡拋出例外。
            Write-Host "[WARN] $verb $($variant.Label) 版本時發生錯誤：$($_.Exception.Message)"
        }
    }

    if (-not $processedAny) {
        Write-Host "[WARN] 在 $PimeRoot 底下找不到任何 PIMETextService.dll（x86／x64／arm64），跳過這一步。"
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

function Remove-BackendEntry {
    param(
        [string]$BackendsJsonPath,
        [string]$Name
    )

    if (-not (Test-Path -LiteralPath $BackendsJsonPath)) {
        Write-Host "[INFO] $BackendsJsonPath 不存在，略過移除 backends.json 項目。"
        return
    }

    $raw = Get-Content -LiteralPath $BackendsJsonPath -Raw -Encoding UTF8
    if (-not $raw -or -not $raw.Trim()) {
        Write-Host "[INFO] $BackendsJsonPath 是空的，略過移除 backends.json 項目。"
        return
    }

    # 單一元素的陣列在 PS5.1 會被 ConvertFrom-Json 解成單一物件，用 @()
    # 強制轉回陣列（跟 ConvertTo-JsonArrayText 的坑是同一類）。
    $entries = @($raw | ConvertFrom-Json)
    $remaining = @($entries | Where-Object { $_.name -ne $Name })

    if ($remaining.Count -eq $entries.Count) {
        Write-Host "[INFO] backends.json 裡沒有「$Name」項目，略過。"
        return
    }

    Write-Host "[INFO] 從 backends.json 移除「$Name」項目。"
    $json = ConvertTo-JsonArrayText -Items $remaining
    Set-Content -LiteralPath $BackendsJsonPath -Value $json -Encoding UTF8
}

# ---- 主流程 ----

if (-not (Test-Admin)) {
    Restart-Elevated
}

$resolvedPimeRoot = Find-PimeRoot -Explicit $PimeRoot
Write-Host "[INFO] PIME 安裝路徑：$resolvedPimeRoot"

$launcherPath = Join-Path $resolvedPimeRoot "PIMELauncher.exe"
$installDir = Join-Path $resolvedPimeRoot $BackendName
$destBackendsJson = Join-Path $resolvedPimeRoot "backends.json"

try {
    Stop-PIMELauncher -Path $launcherPath

    Invoke-PimeTextServiceRegistration -PimeRoot $resolvedPimeRoot -Unregister

    Remove-BackendEntry -BackendsJsonPath $destBackendsJson -Name $BackendName

    if (Test-Path -LiteralPath $installDir) {
        Write-Host "[INFO] 刪除 $installDir ..."
        Remove-Item -LiteralPath $installDir -Recurse -Force
    }
    else {
        Write-Host "[INFO] $installDir 不存在，略過刪除。"
    }

    Invoke-PimeTextServiceRegistration -PimeRoot $resolvedPimeRoot

    if ($RemoveUserData) {
        $userDataDir = if ($Env:APPDATA) { Join-Path $Env:APPDATA "rust-zuyin" } else { $null }
        if ($userDataDir -and (Test-Path -LiteralPath $userDataDir)) {
            Write-Host "[INFO] 刪除使用者自訂詞庫 $userDataDir ..."
            Remove-Item -LiteralPath $userDataDir -Recurse -Force
        }
        else {
            Write-Host "[INFO] 找不到使用者自訂詞庫資料夾，略過。"
        }
    }

    Write-Host "[INFO] 移除完成。"
}
finally {
    Start-PIMELauncher -Path $launcherPath
}

Write-Host ""
Write-Host (
    "[INFO] 請打開「設定 > 時間與語言 > 語言與地區」確認「Rust 注音輸入法」" +
    "已經從輸入法清單消失；如果還在，試著登出再登入、或重新啟動電腦讓 " +
    "Windows 重新整理輸入法清單。其他 PIME 輸入法（新酷音等）應該已經" +
    "恢復正常，如果沒有，同樣試著登出再登入。"
)
