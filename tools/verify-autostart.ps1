# verify-autostart.ps1 — 开机自启验证脚本
#
# 放置于 shell:startup（启动文件夹），用户登录后自动运行：
# 1. 等待 20 秒，让登录自启项完成启动
# 2. 检查 auto-wifi-connector.exe 进程是否存在
# 3. 结果写入 tests\autostart-verify.txt，并截取屏幕存证
# 4. 完成后自动删除本脚本（只验证一次）

$projectRoot = 'C:\Users\liuyu\Desktop\WorkPlace\auto_wifi_connector'
$resultFile = Join-Path $projectRoot 'tests\autostart-verify.txt'
$shotFile = Join-Path $projectRoot 'tests\autostart-verify.png'

Start-Sleep -Seconds 20

$proc = Get-Process auto-wifi-connector -ErrorAction SilentlyContinue
$running = $null -ne $proc
$reg = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Auto WiFi Connector' -ErrorAction SilentlyContinue).'Auto WiFi Connector'

$lines = @(
    "time    : $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')",
    "running : $running",
    "pid     : $($proc.Id -join ',')",
    "registry: $reg",
    "exe exists: $(Test-Path $reg)"
)
$lines | Out-File -FilePath $resultFile -Encoding utf8

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$bmp.Save($shotFile)
$g.Dispose(); $bmp.Dispose()

# 自我删除
Remove-Item -LiteralPath $MyInvocation.MyCommand.Path -Force -ErrorAction SilentlyContinue
