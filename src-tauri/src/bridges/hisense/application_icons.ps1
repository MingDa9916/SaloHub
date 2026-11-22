$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$result = @{}
foreach ($app in (ConvertFrom-Json -InputObject $env:SALOHUB_ICON_REQUEST)) {
    $icon = $null
    $source = $null
    $canvas = $null
    $graphics = $null
    $stream = $null
    try {
        if ($app.logo -and $app.logo -match '^[a-zA-Z]:\\') {
            $directory = [IO.Path]::GetDirectoryName($app.logo)
            $stem = [IO.Path]::GetFileNameWithoutExtension($app.logo)
            $candidate = @($app.logo)
            foreach ($size in @(48, 44, 64, 32, 24, 96, 256)) {
                $candidate += (Join-Path $directory "$stem.targetsize-$size.png")
                $candidate += (Join-Path $directory "$stem.targetsize-$size`_altform-unplated.png")
            }
            $candidate += (Join-Path $directory "$stem.scale-100.png")
            $candidate += (Join-Path $directory "$stem.scale-200.png")
            $logo = $candidate | Where-Object { [IO.File]::Exists($_) } | Select-Object -First 1
            if ($logo) { $source = [Drawing.Image]::FromFile($logo) }
        }
        if (-not $source) {
            if ($app.path -notmatch '^[a-zA-Z]:\\' -or -not [IO.File]::Exists($app.path)) { continue }
            $icon = [Drawing.Icon]::ExtractAssociatedIcon($app.path)
            if (-not $icon) { continue }
            $source = $icon.ToBitmap()
        }
        $canvas = [Drawing.Bitmap]::new(48, 48, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [Drawing.Graphics]::FromImage($canvas)
        $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $scale = 48 / [Math]::Max($source.Width, $source.Height)
        $width = [Math]::Max(1, [int]($source.Width * $scale))
        $height = [Math]::Max(1, [int]($source.Height * $scale))
        $rect = [Drawing.Rectangle]::new([int]((48 - $width) / 2), [int]((48 - $height) / 2), $width, $height)
        $graphics.DrawImage($source, $rect)
        $stream = [IO.MemoryStream]::new()
        $canvas.Save($stream, [Drawing.Imaging.ImageFormat]::Png)
        $result[$app.path] = [Convert]::ToBase64String($stream.ToArray())
    } catch {
        # A single inaccessible program must not prevent other icons from loading.
    } finally {
        foreach ($disposable in @($stream, $graphics, $canvas, $source, $icon)) {
            if ($null -ne $disposable) { $disposable.Dispose() }
        }
    }
}
ConvertTo-Json -InputObject $result -Compress
