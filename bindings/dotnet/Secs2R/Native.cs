using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Text;
using System.Text.Json;

namespace Secs2R;

internal static unsafe class Native
{
    private const int ResultOk = 0;

    internal static ulong EngineNew(string? appSettingsPath)
    {
        byte[] path = Utf8(appSettingsPath);
        fixed (byte* pathPointer = path)
        {
            return ReadHandle(NativeMethods.secs_engine_new(pathPointer, (nuint)path.Length));
        }
    }

    internal static void EngineFree(ulong engine) => NativeMethods.secs_engine_free(engine);

    internal static JsonElement EngineCall(ulong engine, string requestJson)
    {
        byte[] request = Utf8(requestJson);
        fixed (byte* requestPointer = request)
        {
            return ReadData(NativeMethods.secs_engine_call(engine, requestPointer, (nuint)request.Length));
        }
    }

    internal static JsonElement EngineWaitJob(ulong engine, ulong jobId, ulong timeoutMilliseconds) =>
        ReadData(NativeMethods.secs_engine_wait_job(engine, jobId, timeoutMilliseconds));

    internal static ulong EngineSubscribe(ulong engine) =>
        ReadHandle(NativeMethods.secs_engine_subscribe(engine));

    internal static ulong ClientNew(ushort port, string? token, string? tokenPath)
    {
        byte[] tokenBytes = Utf8(token);
        byte[] pathBytes = Utf8(tokenPath);
        fixed (byte* tokenPointer = tokenBytes)
        fixed (byte* pathPointer = pathBytes)
        {
            return ReadHandle(NativeMethods.secs_client_new(
                port,
                tokenPointer,
                (nuint)tokenBytes.Length,
                pathPointer,
                (nuint)pathBytes.Length));
        }
    }

    internal static void ClientFree(ulong client) => NativeMethods.secs_client_free(client);

    internal static JsonElement ClientCall(ulong client, string requestJson)
    {
        byte[] request = Utf8(requestJson);
        fixed (byte* requestPointer = request)
        {
            return ReadData(NativeMethods.secs_client_call(client, requestPointer, (nuint)request.Length));
        }
    }

    internal static JsonElement ClientStatus(ulong client) =>
        ReadData(NativeMethods.secs_client_status(client));

    internal static ulong ClientEvents(ulong client) =>
        ReadHandle(NativeMethods.secs_client_events(client));

    internal static JsonElement EventsNext(ulong subscription, ulong timeoutMilliseconds) =>
        ReadData(NativeMethods.secs_events_next(subscription, timeoutMilliseconds));

    internal static void EventsFree(ulong subscription) =>
        NativeMethods.secs_events_free(subscription);

    private static byte[] Utf8(string? value) =>
        value is null ? Array.Empty<byte>() : Encoding.UTF8.GetBytes(value);

    private static ulong ReadHandle(ulong result) =>
        ReadData(result).GetProperty("handle").GetUInt64();

    private static JsonElement ReadData(ulong result)
    {
        nuint length = NativeMethods.secs_result_len(result);
        if (length == 0)
        {
            throw new InvalidOperationException("The native result handle is unknown.");
        }
        if (length > (nuint)int.MaxValue)
        {
            NativeMethods.secs_result_free(result);
            throw new InvalidOperationException("The native result is too large for a managed array.");
        }

        byte[] bytes = new byte[(int)length];
        int status;
        fixed (byte* buffer = bytes)
        {
            status = NativeMethods.secs_result_take(result, buffer, length);
        }
        if (status != ResultOk)
        {
            NativeMethods.secs_result_free(result);
            throw new InvalidOperationException($"The native result could not be copied (status {status}).");
        }

        using JsonDocument document = JsonDocument.Parse(bytes);
        JsonElement root = document.RootElement;
        if (root.GetProperty("ok").GetBoolean())
        {
            return root.GetProperty("data").Clone();
        }

        JsonElement error = root.GetProperty("error");
        var parameters = new Dictionary<string, string>();
        if (error.TryGetProperty("params", out JsonElement parameterValues))
        {
            foreach (JsonProperty parameter in parameterValues.EnumerateObject())
            {
                parameters.Add(parameter.Name, parameter.Value.GetString() ?? string.Empty);
            }
        }
        throw new SecsException(
            error.GetProperty("code").GetString() ?? "internal",
            error.GetProperty("message").GetString() ?? "SECS 2R returned an error.",
            error.GetProperty("detail").GetString() ?? string.Empty,
            error.TryGetProperty("retryable", out JsonElement retryable) && retryable.GetBoolean(),
            new ReadOnlyDictionary<string, string>(parameters));
    }
}
