using System;
using System.Text.Json;
using System.Threading;

namespace Secs2R;

/// <summary>
/// Embeds the SECS 2R file-based engine. Members may be called concurrently from several threads.
/// </summary>
public sealed class Engine : IDisposable
{
    private long _handle;

    /// <summary>Creates an engine, optionally storing application settings at a specified path.</summary>
    /// <param name="appSettingsPath">The application settings file, or <see langword="null"/> for the default builder.</param>
    public Engine(string? appSettingsPath = null)
    {
        _handle = unchecked((long)Native.EngineNew(appSettingsPath));
    }

    /// <summary>Sends a JSON request and returns the successful response data.</summary>
    /// <param name="request">A request with a <c>method</c> property and any required parameters.</param>
    /// <returns>The response value inside the successful native envelope.</returns>
    public JsonElement Call(JsonElement request) => CallCore(request.GetRawText());

    /// <summary>Sends JSON request text and returns the successful response data as JSON text.</summary>
    /// <param name="requestJson">A JSON request with a <c>method</c> property and any required parameters.</param>
    /// <returns>The response value inside the successful native envelope.</returns>
    public string Call(string requestJson)
    {
        if (requestJson is null)
        {
            throw new ArgumentNullException(nameof(requestJson));
        }
        return CallCore(requestJson).GetRawText();
    }

    /// <summary>Waits for a job's terminal event.</summary>
    /// <param name="jobId">The service-assigned job identifier.</param>
    /// <param name="timeout">The maximum time to wait.</param>
    /// <returns>The terminal job event.</returns>
    public JsonElement WaitJob(ulong jobId, TimeSpan timeout)
    {
        ulong handle = GetHandle();
        try
        {
            return Native.EngineWaitJob(handle, jobId, TimeoutMilliseconds(timeout));
        }
        finally
        {
            GC.KeepAlive(this);
        }
    }

    /// <summary>
    /// Subscribes to job events now and returns the stream: every event emitted after this call
    /// returns reaches it, so call it before starting the job to watch.
    /// </summary>
    /// <param name="cancellationToken">Stops polling and frees the native subscription.</param>
    /// <returns>A stream of job-event JSON values that also reports dropped events.</returns>
    public EventStream Events(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return new EventStream(Subscribe(), cancellationToken);
    }

    /// <summary>Releases the embedded engine handle. Calling this method more than once is harmless.</summary>
    public void Dispose()
    {
        FreeHandle();
        GC.SuppressFinalize(this);
    }

    private JsonElement CallCore(string requestJson)
    {
        ulong handle = GetHandle();
        try
        {
            return Native.EngineCall(handle, requestJson);
        }
        finally
        {
            GC.KeepAlive(this);
        }
    }

    private ulong Subscribe()
    {
        ulong handle = GetHandle();
        try
        {
            return Native.EngineSubscribe(handle);
        }
        finally
        {
            GC.KeepAlive(this);
        }
    }

    private ulong GetHandle()
    {
        long handle = Volatile.Read(ref _handle);
        if (handle == 0)
        {
            throw new ObjectDisposedException(nameof(Engine));
        }
        return unchecked((ulong)handle);
    }

    private void FreeHandle()
    {
        long handle = Interlocked.Exchange(ref _handle, 0);
        if (handle != 0)
        {
            Native.EngineFree(unchecked((ulong)handle));
        }
    }

    private static ulong TimeoutMilliseconds(TimeSpan timeout)
    {
        if (timeout < TimeSpan.Zero)
        {
            throw new ArgumentOutOfRangeException(nameof(timeout), "The timeout cannot be negative.");
        }
        return checked((ulong)(timeout.Ticks / TimeSpan.TicksPerMillisecond));
    }

    /// <summary>Releases the native handle if the engine was not disposed.</summary>
    ~Engine() => FreeHandle();
}
