using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net;
using System.Net.Sockets;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Secs2R;
using Xunit;

namespace Secs2R.Tests;

public sealed class PackageTests
{
    [Fact]
    public void PackageAndBootstrapVersionsMatchTheNativeCrate()
    {
        using var temp = new TestDirectory();
        using var engine = new Engine(temp.SettingsPath);

        string expected = CrateVersion();
        string informational = typeof(Engine).Assembly
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()!
            .InformationalVersion
            .Split('+')[0];
        JsonElement bootstrap = Call(engine, "bootstrap");

        Assert.Equal(expected, informational);
        Assert.Equal(expected, bootstrap.GetProperty("data").GetProperty("version").GetString());
    }

    [Fact]
    public async Task BootstrapSucceedsAndEventsCanBeCancelled()
    {
        using var temp = new TestDirectory();
        using var engine = new Engine(temp.SettingsPath);
        JsonElement bootstrap = Call(engine, "bootstrap");
        Assert.Equal("bootstrap", bootstrap.GetProperty("method").GetString());

        using var cancellation = new CancellationTokenSource();
        EventStream stream = engine.Events(cancellation.Token);
        await using IAsyncEnumerator<JsonElement> events = stream.GetAsyncEnumerator();
        Task<bool> pending = events.MoveNextAsync().AsTask();
        await Task.Delay(300);
        cancellation.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => pending);
        Assert.Equal(0UL, stream.DroppedEvents);
        Assert.Throws<InvalidOperationException>(() => stream.GetAsyncEnumerator());
    }

    [Fact]
    public async Task DisposingAStreamEndsItsEnumeration()
    {
        using var temp = new TestDirectory();
        using var engine = new Engine(temp.SettingsPath);
        EventStream stream = engine.Events();
        await using IAsyncEnumerator<JsonElement> events = stream.GetAsyncEnumerator();
        Task<bool> pending = events.MoveNextAsync().AsTask();
        await Task.Delay(300);

        stream.Dispose();

        Assert.False(await pending.WaitAsync(TimeSpan.FromSeconds(5)));
    }

    [Fact]
    public void FreshEngineResultsHaveStructuredErrorFields()
    {
        using var temp = new TestDirectory();
        using var engine = new Engine(temp.SettingsPath);

        SecsException error = Assert.Throws<SecsException>(() => Call(engine, "results"));

        Assert.Equal("project_not_open", error.Code);
        Assert.False(string.IsNullOrWhiteSpace(error.Message));
        Assert.False(string.IsNullOrWhiteSpace(error.Detail));
    }

    [Fact]
    public void SecsExceptionCarriesParams()
    {
        using var temp = new TestDirectory();
        using var engine = new Engine(temp.SettingsPath);
        SecsException error = Assert.Throws<SecsException>(() => Call(engine, "results"));
        Assert.Equal("project_not_open", error.Code);
        var property = typeof(SecsException).GetProperty("Params");
        Assert.NotNull(property);
        var parameters = Assert.IsAssignableFrom<IReadOnlyDictionary<string, string>>(property.GetValue(error));
        Assert.Empty(parameters);
        Assert.False(error.Retryable);
        Assert.StartsWith("project_not_open: ", error.Message);
    }

    [Fact]
    public void ClosedPortClientFailsWithTransportErrorWithinFiveSeconds()
    {
        int port = ClosedPort();
        using var client = new Client(port, token: "test");
        var started = Stopwatch.StartNew();

        SecsException error = Assert.Throws<SecsException>(() => Call(client, "bootstrap"));

        Assert.Equal("transport_error", error.Code);
        Assert.True(started.Elapsed < TimeSpan.FromSeconds(5), $"Call took {started.Elapsed}.");
    }

    [Fact]
    public async Task ClientEventsOnAClosedPortFailWithTransportError()
    {
        int port = ClosedPort();
        using var client = new Client(port, token: "test");

        SecsException connecting = Assert.Throws<SecsException>(() => client.Events());
        SecsException connectingAsync = await Assert.ThrowsAsync<SecsException>(() => client.EventsAsync());

        Assert.Equal("transport_error", connecting.Code);
        Assert.Equal("transport_error", connectingAsync.Code);
    }

    [Fact]
    public async Task EventStreamPassesLaggedThrough()
    {
        var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        int port = ((IPEndPoint)listener.LocalEndpoint).Port;
        Task server = Task.Run(async () =>
        {
            using TcpClient accepted = await listener.AcceptTcpClientAsync();
            using NetworkStream stream = accepted.GetStream();
            var request = new List<byte>();
            var buffer = new byte[4096];
            while (!Encoding.ASCII.GetString(request.ToArray()).Contains("\r\n\r\n", StringComparison.Ordinal))
            {
                int read = await stream.ReadAsync(buffer, 0, buffer.Length);
                request.AddRange(buffer.Take(read));
            }
            string headers = Encoding.ASCII.GetString(request.ToArray());
            string key = headers.Split(new[] { "\r\n" }, StringSplitOptions.None)
                .Single(line => line.StartsWith("Sec-WebSocket-Key:", StringComparison.OrdinalIgnoreCase))
                .Split(':', 2)[1].Trim();
            byte[] digest = SHA1.HashData(Encoding.ASCII.GetBytes(
                key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"));
            string accept = Convert.ToBase64String(digest);
            byte[] response = Encoding.ASCII.GetBytes(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n" +
                "Connection: Upgrade\r\nSec-WebSocket-Accept: " + accept + "\r\n\r\n");
            await stream.WriteAsync(response, 0, response.Length);
            byte[] payload = Encoding.UTF8.GetBytes("{\"kind\":\"lagged\",\"missed\":7}");
            var frame = new byte[payload.Length + 2];
            frame[0] = 0x81;
            frame[1] = checked((byte)payload.Length);
            payload.CopyTo(frame, 2);
            await stream.WriteAsync(frame, 0, frame.Length);
            await stream.FlushAsync();
            await Task.Delay(200);
        });

        using var client = new Client(port, token: "test");
        await using EventStream events = client.Events();
        await using IAsyncEnumerator<JsonElement> enumerator = events.GetAsyncEnumerator();
        Assert.True(await enumerator.MoveNextAsync());
        JsonElement lagged = enumerator.Current;
        Assert.Equal("lagged", lagged.GetProperty("kind").GetString());
        Assert.Equal(7UL, lagged.GetProperty("event").GetProperty("missed").GetUInt64());
        events.Dispose();
        await server;
        listener.Stop();
    }

    [Fact]
    public async Task MockMeasurementCalibratesAndExportsEqualizerApoFiles()
    {
        using var temp = new TestDirectory();
        string projectDirectory = Path.Combine(temp.Path, "project");
        string projectPath = Path.Combine(projectDirectory, "mock.secs");
        string exportPath = Path.Combine(temp.Path, "equalizer-apo");
        Directory.CreateDirectory(projectDirectory);
        Directory.CreateDirectory(exportPath);
        using var engine = new Engine(temp.SettingsPath);

        Call(engine, "project_new", new Dictionary<string, object?>
        {
            ["path"] = projectPath,
            ["preset"] = "7.1.4",
            ["sub_count"] = 3,
            ["sample"] = true,
            ["tutorial"] = false,
        });
        Call(engine, "measure_configure", new Dictionary<string, object?>
        {
            ["method"] = "mock",
        });
        WaitForJob(engine, Call(engine, "measure_position", new Dictionary<string, object?>
        {
            ["position"] = 1,
        }));
        Call(engine, "project_save", new Dictionary<string, object?>());

        // The stream subscribes before the job starts, so it sees the job's terminal event.
        await using (EventStream events = engine.Events())
        {
            ulong job = Call(engine, "calibrate", new Dictionary<string, object?>())
                .GetProperty("data").GetProperty("job").GetUInt64();
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(120));
            string? status = null;
            await foreach (JsonElement publicEvent in events.WithCancellation(timeout.Token))
            {
                if (publicEvent.GetProperty("kind").GetString() != "job")
                {
                    continue;
                }
                JsonElement jobEvent = publicEvent.GetProperty("event");
                if (jobEvent.GetProperty("event").GetString() == "finished"
                    && jobEvent.GetProperty("job").GetUInt64() == job)
                {
                    status = jobEvent.GetProperty("status").GetString();
                    break;
                }
            }
            Assert.Equal("succeeded", status);
        }

        JsonElement results = Call(engine, "results");
        Assert.NotEqual(JsonValueKind.Null, results.GetProperty("data").GetProperty("calibration").ValueKind);

        WaitForJob(engine, Call(engine, "export", new Dictionary<string, object?>
        {
            ["kind"] = "equalizer_apo",
            ["folder"] = exportPath,
        }));
        Assert.NotEmpty(Directory.EnumerateFiles(exportPath, "*", SearchOption.AllDirectories));
    }

    [Fact]
    public void DisposedEngineRejectsCalls()
    {
        using var temp = new TestDirectory();
        var engine = new Engine(temp.SettingsPath);
        engine.Dispose();

        Assert.Throws<ObjectDisposedException>(() => Call(engine, "bootstrap"));
    }

    private static JsonElement Call(Engine engine, string method, object? parameters = null)
    {
        var request = new Dictionary<string, object?> { ["method"] = method };
        if (parameters is not null)
        {
            request["params"] = parameters;
        }
        return engine.Call(JsonSerializer.SerializeToElement(request));
    }

    private static JsonElement Call(Client client, string method, object? parameters = null)
    {
        var request = new Dictionary<string, object?> { ["method"] = method };
        if (parameters is not null)
        {
            request["params"] = parameters;
        }
        return client.Call(JsonSerializer.SerializeToElement(request));
    }

    private static void WaitForJob(Engine engine, JsonElement started)
    {
        ulong job = started.GetProperty("data").GetProperty("job").GetUInt64();
        JsonElement finished = engine.WaitJob(job, TimeSpan.FromSeconds(120));
        Assert.Equal("finished", finished.GetProperty("event").GetString());
        Assert.Equal("succeeded", finished.GetProperty("status").GetString());
    }

    private static string CrateVersion() =>
        typeof(PackageTests).Assembly
            .GetCustomAttributes<AssemblyMetadataAttribute>()
            .Single(attribute => attribute.Key == "Secs2RCrateVersion")
            .Value!;

    private static int ClosedPort()
    {
        var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        int port = ((IPEndPoint)listener.LocalEndpoint).Port;
        listener.Stop();
        return port;
    }

    private sealed class TestDirectory : IDisposable
    {
        internal TestDirectory()
        {
            Path = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                $"secs2r-dotnet-{Guid.NewGuid():N}");
            Directory.CreateDirectory(Path);
        }

        internal string Path { get; }

        internal string SettingsPath => System.IO.Path.Combine(Path, "settings.json");

        public void Dispose()
        {
            try
            {
                Directory.Delete(Path, recursive: true);
            }
            catch (IOException)
            {
            }
            catch (UnauthorizedAccessException)
            {
            }
        }
    }
}
