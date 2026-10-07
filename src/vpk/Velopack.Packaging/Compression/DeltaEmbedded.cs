using Microsoft.Extensions.Logging;
using Velopack.Core;

namespace Velopack.Packaging.Compression;

public class DeltaEmbedded
{
    private readonly DeltaImpl _delta;

    public DeltaEmbedded(string hdiffzPath, ILogger logger, string baseTmpDir)
    {
        _delta = new DeltaImpl(hdiffzPath, logger, baseTmpDir);
    }

    public void ApplyDeltaPackageFast(string workingPath, string deltaPackageZip, Action<int> progress = null)
    {
        _delta.ApplyDeltaPackageFast(workingPath, deltaPackageZip, progress);
    }

    private class DeltaImpl : DeltaPackage
    {
        private readonly HDiffPatch _hdiff;

        public DeltaImpl(string hdiffzPath, ILogger logger, string baseTmpDir) : base(logger.ToVelopackLogger(), baseTmpDir)
        {
            _hdiff = new HDiffPatch(hdiffzPath);
        }

        protected override void ApplyZstdPatch(string baseFile, string patchFile, string outputFile)
        {
            // Resolve zstd only when reading a previously published delta.
            new Zstd(HelperFile.GetZstdPath()).ApplyPatch(baseFile, patchFile, outputFile);
        }

        protected override void ApplyHDiffPatch(string baseFile, string patchFile, string outputFile)
            => _hdiff.ApplyPatch(baseFile, patchFile, outputFile);
    }
}
