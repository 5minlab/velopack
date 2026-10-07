using Velopack.Core;
using Velopack.Packaging.Compression;

namespace Velopack.Packaging;

public class HDiffPatch
{
    private readonly string _hdiffzExe;

    public HDiffPatch(string hdiffzExe) => _hdiffzExe = hdiffzExe;

    public void CreatePatch(string oldFile, string newFile, string outputFile, DeltaMode mode)
    {
        var matching = mode switch {
            DeltaMode.BestSpeed => "-s-64",
            DeltaMode.BestSize => "-m-4",
            _ => throw new ArgumentException("A delta compression mode is required.", nameof(mode)),
        };

        // HDIFF13 without internal compression: the enclosing nupkg compresses the patch.
        // Keep a single worker per file; DeltaPackageBuilder already parallelizes files.
        // The default hdiffz verification pass checks the generated patch against newFile.
        Exe.InvokeAndThrowIfNonZero(_hdiffzExe, new List<string> {
            "-f", "-p-1", matching, oldFile, newFile, outputFile,
        }, null);
        // Fail at packaging time if a different helper version changes its default format.
        using var patch = File.OpenRead(outputFile);
        Span<byte> header = stackalloc byte[9];
        patch.ReadExactly(header);
        if (!header.SequenceEqual("HDIFF13&\0"u8))
            throw new InvalidDataException("Expected an uncompressed HDIFF13 patch. Use HDiffPatch v4.12.0.");
    }

    public void ApplyPatch(string oldFile, string patchFile, string outputFile)
    {
        Exe.InvokeAndThrowIfNonZero(_hdiffzExe, new List<string> {
            "--patch", "-f", oldFile, patchFile, outputFile,
        }, null);
    }
}
