# puts the app icon into a built windows exe (scripts/build.mjs) - no resource compiler, no npm packages: System.Drawing scales it, UpdateResource writes it
# usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows_icon.ps1 -Exe <app.exe> -Png <icon.png>
param([Parameter(Mandatory)][string]$Exe, [Parameter(Mandatory)][string]$Png)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.IO;
using System.Runtime.InteropServices;

public static class ExeIcon
{
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr BeginUpdateResource(string file, bool deleteExisting);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool UpdateResource(IntPtr handle, IntPtr type, IntPtr name, ushort language, byte[] data, uint size);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool EndUpdateResource(IntPtr handle, bool discard);

    const int RT_ICON = 3;
    const int RT_GROUP_ICON = 14;

    // one RT_ICON per size (png data) + the RT_GROUP_ICON that lists them - explorer takes the first group
    public static void Set(string exe, byte[][] images, int[] sizes)
    {
        IntPtr handle = BeginUpdateResource(exe, false);
        if (handle == IntPtr.Zero) throw new Exception("BeginUpdateResource failed: " + Marshal.GetLastWin32Error());

        var group = new MemoryStream();
        var writer = new BinaryWriter(group);
        writer.Write((ushort)0);
        writer.Write((ushort)1);
        writer.Write((ushort)images.Length);

        for (int i = 0; i < images.Length; i++)
        {
            byte[] data = images[i];
            if (!UpdateResource(handle, (IntPtr)RT_ICON, (IntPtr)(i + 1), 0, data, (uint)data.Length)) throw new Exception("UpdateResource (icon) failed: " + Marshal.GetLastWin32Error());

            byte size = (byte)(sizes[i] >= 256 ? 0 : sizes[i]);
            writer.Write(size);
            writer.Write(size);
            writer.Write((byte)0);
            writer.Write((byte)0);
            writer.Write((ushort)1);
            writer.Write((ushort)32);
            writer.Write((uint)data.Length);
            writer.Write((ushort)(i + 1));
        }

        byte[] groupData = group.ToArray();
        if (!UpdateResource(handle, (IntPtr)RT_GROUP_ICON, (IntPtr)1, 0, groupData, (uint)groupData.Length)) throw new Exception("UpdateResource (group) failed: " + Marshal.GetLastWin32Error());
        if (!EndUpdateResource(handle, false)) throw new Exception("EndUpdateResource failed: " + Marshal.GetLastWin32Error());
    }
}
"@

$sizes = @(256, 128, 64, 48, 32, 16)
$source = [System.Drawing.Image]::FromFile((Resolve-Path $Png))
$images = @()

foreach ($size in $sizes)
{
    $bitmap = New-Object System.Drawing.Bitmap $size, $size
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.DrawImage($source, 0, 0, $size, $size)
    $graphics.Dispose()

    $stream = New-Object System.IO.MemoryStream
    $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
    $images += ,$stream.ToArray()
}

$source.Dispose()
[ExeIcon]::Set((Resolve-Path $Exe).Path, [byte[][]]$images, [int[]]$sizes)
