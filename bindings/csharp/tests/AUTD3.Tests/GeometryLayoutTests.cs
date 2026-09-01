using System;
using System.IO;
using System.Numerics;
using AUTD3;
using Xunit;

namespace AUTD3.Tests
{
    public class GeometryLayoutTests
    {
        private const string Fixture = @"[
  { ""origin"": [0.0, 0.0, 0.0] },
  { ""origin"": [192.0, 0.0, 0.0], ""rotation"": [0.7071068, 0.0, 0.7071068, 0.0] }
]";

        private static Geometry TwoDevices() => new Geometry(new[]
        {
            new Autd3(Vector3.Zero),
            new Autd3(new Vector3(192f, 0f, 0f), Quaternion.CreateFromAxisAngle(Vector3.UnitY, MathF.PI / 2f)),
        });

        [Fact]
        public void FromJsonBuildsTheDeclaredPlacement()
        {
            using var geometry = Geometry.FromJson(Fixture);
            Assert.Equal(2, geometry.NumDevices);
            Assert.Equal(Vector3.Zero, geometry[0].Position(0));
            Assert.Equal(new Vector3(192f, 0f, 0f), geometry[1].Position(0));

            var rotated = geometry[1].Position(1);
            Assert.Equal(192f, rotated.X, 3);
            Assert.Equal(0f, rotated.Y, 3);
            Assert.Equal(-Autd3.PitchMm, rotated.Z, 3);

            var axial = geometry[1].AxialDirection;
            Assert.Equal(1f, axial.X, 3);
            Assert.Equal(0f, axial.Y, 3);
            Assert.Equal(0f, axial.Z, 3);
        }

        [Fact]
        public void ToJsonRoundTripsThroughFromJson()
        {
            using var geometry = TwoDevices();
            var json = geometry.ToJson();

            using var restored = Geometry.FromJson(json);
            Assert.Equal(2, restored.NumDevices);
            Assert.Equal(new Vector3(192f, 0f, 0f), restored[1].Position(0));
            Assert.Equal(1f, restored[1].AxialDirection.X, 3);
        }

        [Fact]
        public void ToJsonRoundTripsThroughAFile()
        {
            var path = Path.Combine(Path.GetTempPath(), $"autd3-geometry-layout-{Guid.NewGuid():N}.json");
            try
            {
                using var geometry = TwoDevices();
                File.WriteAllText(path, geometry.ToJson());

                using var restored = Geometry.FromJson(File.ReadAllText(path));
                Assert.Equal(2, restored.NumDevices);
                Assert.Equal(new Vector3(192f, 0f, 0f), restored[1].Position(0));
            }
            finally
            {
                File.Delete(path);
            }
        }

        [Fact]
        public void AnUnknownFieldIsRejected()
        {
            var e = Assert.Throws<Autd3Exception>(() =>
                Geometry.FromJson(@"[{""origin"": [0,0,0], ""rotaton"": [1,0,0,0]}]"));
            Assert.Contains("rotaton", e.Message);
        }

        [Fact]
        public void ANonNormalizedRotationIsRejected()
        {
            var e = Assert.Throws<Autd3Exception>(() =>
                Geometry.FromJson(@"[{""origin"": [0,0,0], ""rotation"": [1,1,0,0]}]"));
            Assert.Contains("unit quaternion", e.Message);
        }

        [Fact]
        public void AWrongLengthOriginIsRejected()
        {
            var e = Assert.Throws<Autd3Exception>(() => Geometry.FromJson(@"[{""origin"": [0,0]}]"));
            Assert.Contains("length", e.Message);
        }
    }
}
