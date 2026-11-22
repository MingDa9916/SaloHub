$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$items = [System.Collections.Generic.List[object]]::new()
function Add-App([string]$name, [string]$path, [string]$source, [string]$launchId = '', [string]$logo = '') {
    if (-not $path) { return }
    $path = [Environment]::ExpandEnvironmentVariables($path.Trim().Trim('"'))
    if (-not $path.EndsWith('.exe', [StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $path -PathType Leaf)) { return }
    $exe = [IO.Path]::GetFileName($path)
    if ($exe -match '(?i)^(unins|uninstall|setup|update|crash|maintenancetool|vc_redist)') { return }
    if ([string]::IsNullOrWhiteSpace($name)) { $name = [IO.Path]::GetFileNameWithoutExtension($path) }
    $items.Add([pscustomobject]@{name=$name;process_name=$exe;path=$path;source=$source;launch_id=$launchId;logo=$logo})
}
$shell = New-Object -ComObject WScript.Shell
$roots = @([Environment]::GetFolderPath('StartMenu'), [Environment]::GetFolderPath('CommonStartMenu'), [Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('CommonDesktopDirectory')) | Select-Object -Unique
foreach ($root in $roots) {
    if (-not $root) { continue }
    Get-ChildItem -LiteralPath $root -Filter '*.lnk' -Recurse -File | ForEach-Object {
        $link = $shell.CreateShortcut($_.FullName)
        Add-App $_.BaseName $link.TargetPath '开始菜单 / 桌面'
    }
}
foreach ($root in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\App Paths', 'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths')) {
    Get-ChildItem $root | ForEach-Object { $v = $_.GetValue(''); Add-App $_.PSChildName $v '注册应用' }
}
foreach ($root in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*')) {
    Get-ItemProperty $root | Where-Object { $_.DisplayName -and -not $_.SystemComponent } | ForEach-Object {
        $app = $_
        $icon = ($app.DisplayIcon -replace ',\s*-?\d+$','').Trim('"')
        if ($icon) { Add-App $app.DisplayName $icon '已安装应用' }
        if ($app.InstallLocation -and (Test-Path -LiteralPath $app.InstallLocation -PathType Container)) {
            Get-ChildItem -LiteralPath $app.InstallLocation -Filter '*.exe' -File -Depth 2 | Select-Object -First 150 | ForEach-Object {
                $name = $_.VersionInfo.FileDescription
                if ([string]::IsNullOrWhiteSpace($name)) { $name = $_.BaseName }
                Add-App $name $_.FullName '已安装应用 / 游戏'
            }
        }
    }
}
Get-AppxPackage | ForEach-Object {
    $package = $_
    $manifest = Get-AppxPackageManifest -Package $package.PackageFullName
    foreach ($app in $manifest.Package.Applications.Application) {
        if ($app.Executable -and $app.Executable -notmatch '^\$') {
            $logo = ''
            $relativeLogo = [string]$app.VisualElements.Square44x44Logo
            if ($relativeLogo -match '(?i)^[\w .\\/-]+\.png$' -and $relativeLogo -notmatch '^[\\/]|(?:^|[\\/])\.\.(?:[\\/]|$)') {
                $logo = Join-Path $package.InstallLocation $relativeLogo
            }
            Add-App $package.Name (Join-Path $package.InstallLocation $app.Executable) '商店应用' ($package.PackageFamilyName + '!' + $app.Id) $logo
        }
    }
}
Get-Process | Where-Object Path | ForEach-Object { Add-App $_.ProcessName $_.Path '运行中的应用' }
# Steam's game manifest gives the actual library path; no full-drive traversal.
$steam = (Get-ItemProperty 'HKCU:\Software\Valve\Steam').SteamPath
if ($steam) {
    $libraries = [System.Collections.Generic.List[string]]::new(); $libraries.Add($steam)
    $vdf = Get-Content -LiteralPath (Join-Path $steam 'steamapps\libraryfolders.vdf') -Raw
    foreach ($m in [regex]::Matches($vdf, '"path"\s+"([^"]+)"')) { $libraries.Add($m.Groups[1].Value.Replace('\\','\')) }
    foreach ($lib in $libraries | Select-Object -Unique) {
        Get-ChildItem -LiteralPath (Join-Path $lib 'steamapps') -Filter 'appmanifest_*.acf' -File | ForEach-Object {
            $data = Get-Content -LiteralPath $_.FullName -Raw
            $folder = [regex]::Match($data, '"installdir"\s+"([^"]+)"').Groups[1].Value
            if ($folder) { Get-ChildItem -LiteralPath (Join-Path $lib "steamapps\common\$folder") -Filter '*.exe' -File -Depth 3 | Select-Object -First 150 | ForEach-Object { Add-App $_.BaseName $_.FullName 'Steam 游戏' } }
        }
    }
}
$result = @($items | Sort-Object path -Unique | Sort-Object name | Select-Object -First 10000)
ConvertTo-Json -InputObject $result -Depth 3 -Compress
