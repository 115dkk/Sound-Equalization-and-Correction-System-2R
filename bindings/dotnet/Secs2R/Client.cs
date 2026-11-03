using System;
using System.Text.Json;
using System.Threading;

namespace Secs2R;

/// <summary>
/// Controls a running SECS 2R application through its authenticated local API.
/// Members may be called concurrently from several threads.
/// </summary>
public sealed class Client : IDisposable
{
    private long _handle;

    /// <summary>The local API's default loopback port.</summary>
    public const int DefaultPort = 4712;

    /// <summary>Creates a client using a token value or token file.</summary>
    /// <param name="port">The local API loopback port.</param>
    /// <param name="token">A bearer token, or <see langword="null"/> to read a token file.</param>
    /// <param name="tokenPath">A token file, or <see langword="null"/> for the application's default token file.</param>
    public Client(int port = DefaultPort, string? token = null, string? tokenPath = null)
    {
        if (port is < ushort.MinValue or > ushort.MaxValue)
        {
            throw new ArgumentOutOfRangeException(nameof(port));
        }
        _handle = unchecked((long)Native.ClientNew((ushort)port, token, tokenPath));
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

    /// <summary>Reads the running application's aggregate local-API status.</summary>
    /// <returns>The status JSON value.</returns>
    public JsonElement Status()
    {
        ulong handle = GetHandle();
        try
        {
            return Native.ClientStatus(handle);
        }
        finally
        {
            GC.KeepAlive(this);
        }
    }

    /// <summary>Streams the local-API events sent after each enumeration starts.</summary>
    /// <param name="cancellationToken">Stops polling and frees the native subscription.</param>
    /// <returns>A stream of server-event JSON values that also reports dropped events.</returns>
    public EventStream Events(CancellationToken cancellationToken = default) =>
        new EventStream(Subscribe, cancellationToken);

    /// <summary>Releases the client handle. Calling this method more than once is harmless.</summary>
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
            return Native.ClientCall(handle, requestJson);
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
            return Native.ClientEvents(handle);
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
            throw new ObjectDisposedException(nameof(Client));
        }
        return unchecked((ulong)handle);
    }

    private void FreeHandle()
    {
        long handle = Interlocked.Exchange(ref _handle, 0);
        if (handle != 0)
        {
            Native.ClientFree(unchecked((ulong)handle));
        }
    }

    /// <summary>Releases the native handle if the client was not disposed.</summary>
    ~Client() => FreeHandle();
}
