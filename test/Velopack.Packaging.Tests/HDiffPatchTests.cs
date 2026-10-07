using System.IO.Compression;
using Velopack.Core;
using Velopack.Packaging.Compression;
using Velopack.Util;

namespace Velopack.Packaging.Tests;

public class HDiffPatchTests(ITestOutputHelper output)
{
    [Theory]
    [InlineData(DeltaMode.BestSpeed, "old content", "new content")]
    [InlineData(DeltaMode.BestSize, "old content", "new content")]
    [InlineData(DeltaMode.BestSpeed, "", "new content")]
    [InlineData(DeltaMode.BestSize, "old content", "")]
    public void RoundTrip(DeltaMode mode, string oldText, string newText)
    {
        using var temp = TempUtil.GetTempDirectory(out var dir);
        var oldFile = Path.Combine(dir, "old 한 글.txt");
        var newFile = Path.Combine(dir, "new 한 글.txt");
        var patch = Path.Combine(dir, "patch.hdiff");
        var restored = Path.Combine(dir, "restored.txt");
        File.WriteAllText(oldFile, oldText);
        File.WriteAllText(newFile, newText);
        var hdiff = new HDiffPatch(HelperFile.GetHDiffPatchPath());
        hdiff.CreatePatch(oldFile, newFile, patch, mode);
        hdiff.ApplyPatch(oldFile, patch, restored);
        Assert.Equal(File.ReadAllBytes(newFile), File.ReadAllBytes(restored));
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void PackageRoundTripAndChecksumFailure(bool corruptChecksum)
    {
        using var temp = TempUtil.GetTempDirectory(out var dir);
        using var logger = output.BuildLoggerFor<HDiffPatchTests>();
        var oldDir = Path.Combine(dir, "old");
        var newDir = Path.Combine(dir, "new");
        foreach (var root in new[] { oldDir, newDir }) {
            Directory.CreateDirectory(Path.Combine(root, "lib", "app"));
            File.WriteAllText(Path.Combine(root, "lib", "app", "same"), "same");
            File.WriteAllText(Path.Combine(root, "lib", "app", "changed"), root);
            File.WriteAllText(Path.Combine(root, "lib", "app", "empty"), root == oldDir ? "old" : "");
            File.WriteAllText(Path.Combine(root, "test.nuspec"),
                $"<package><metadata><id>test</id><version>{(root == oldDir ? "1.0.0" : "2.0.0")}</version></metadata></package>");
        }
        File.WriteAllText(Path.Combine(oldDir, "lib", "app", "removed"), "remove");
        File.WriteAllText(Path.Combine(newDir, "lib", "app", "added"), "add");
        var oldZip = Path.Combine(dir, "test-1.0.0-full.nupkg");
        var newZip = Path.Combine(dir, "test-2.0.0-full.nupkg");
        var deltaZip = Path.Combine(dir, "test-2.0.0-delta.nupkg");
        ZipFile.CreateFromDirectory(oldDir, oldZip);
        ZipFile.CreateFromDirectory(newDir, newZip);
        new DeltaPackageBuilder(logger).CreateDeltaPackage(new ReleasePackage(oldZip), new ReleasePackage(newZip),
            deltaZip, DeltaMode.BestSpeed, _ => { });
        using (var zip = ZipFile.Open(deltaZip, ZipArchiveMode.Update)) {
            Assert.NotNull(zip.GetEntry("lib/app/changed.hdiff"));
            Assert.DoesNotContain(zip.Entries, e => e.FullName.EndsWith(".zsdiff"));
            if (corruptChecksum) {
                var entry = zip.GetEntry("lib/app/changed.shasum")!;
                entry.Delete();
                using var writer = new StreamWriter(zip.CreateEntry("lib/app/changed.shasum").Open());
                writer.Write($"{new string('0', 40)} changed.shasum {new FileInfo(Path.Combine(newDir, "lib", "app", "changed")).Length}");
            }
        }
        var delta = new DeltaEmbedded(HelperFile.GetHDiffPatchPath(), logger, dir);
        if (corruptChecksum) {
            Assert.Throws<InvalidDataException>(() => delta.ApplyDeltaPackageFast(oldDir, deltaZip));
            Assert.Equal(oldDir, File.ReadAllText(Path.Combine(oldDir, "lib", "app", "changed")));
        } else {
            delta.ApplyDeltaPackageFast(oldDir, deltaZip);
            var files = Directory.GetFiles(newDir, "*", SearchOption.AllDirectories);
            Assert.Equal(files.Length, Directory.GetFiles(oldDir, "*", SearchOption.AllDirectories).Length);
            foreach (var file in files)
                Assert.Equal(File.ReadAllBytes(file), File.ReadAllBytes(Path.Combine(oldDir, Path.GetRelativePath(newDir, file))));
        }
    }
}
