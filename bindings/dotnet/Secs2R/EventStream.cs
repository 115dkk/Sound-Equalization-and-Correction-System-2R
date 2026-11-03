using System;
using System.Collections.Generic;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace Secs2R;

/// <summary>
/// Events from an <see cref="Engine"/> or a <see cref="Client"/>. Each enumeration opens its own
/// native subscription, receives the events emitted after it starts, and frees the subscription
/// when the enumeration ends.
/// </summary>
public sealed class EventStream : IAsyncEnumerable<JsonElement>
{
    private const ulong PollMilliseconds = 200;
    private readonly Func<ulong> _subscribe;
    private readonly CancellationToken _cancellationToken;
    private long _droppedEvents;

    internal EventStream(Func<ulong> subscribe, CancellationToken cancellationToken)
    {
        _subscribe = subscribe;
        _cancellationToken = cancellationToken;
    }

    /// <summary>
    /// Gets how many events the native queue dropped because the latest enumeration read them too
    /// slowly, as of its latest poll. A dropped event can be a job's terminal event, so a caller
    /// that sees this grow should ask for the job's state, for example with
    /// <see cref="Engine.WaitJob"/>.
    /// </summary>
    public ulong DroppedEvents => unchecked((ulong)Interlocked.Read(ref _droppedEvents));

    /// <summary>Opens a native subscription and polls it until the stream closes or is cancelled.</summary>
    /// <param name="cancellationToken">Stops polling and frees the native subscription.</param>
    /// <returns>An enumerator of event JSON values.</returns>
    public async IAsyncEnumerator<JsonElement> GetAsyncEnumerator(CancellationToken cancellationToken = default)
    {
        using CancellationTokenSource linked =
            CancellationTokenSource.CreateLinkedTokenSource(_cancellationToken, cancellationToken);
        CancellationToken token = linked.Token;
        token.ThrowIfCancellationRequested();
        ulong subscription = _subscribe();
        try
        {
            while (true)
            {
                token.ThrowIfCancellationRequested();
                JsonElement poll = await Task.Run(
                    () => Native.EventsNext(subscription, PollMilliseconds),
                    CancellationToken.None).ConfigureAwait(false);
                Interlocked.Exchange(ref _droppedEvents, unchecked((long)poll.GetProperty("dropped").GetUInt64()));
                token.ThrowIfCancellationRequested();
                if (poll.GetProperty("closed").GetBoolean())
                {
                    yield break;
                }
                JsonElement eventValue = poll.GetProperty("event");
                if (eventValue.ValueKind != JsonValueKind.Null)
                {
                    yield return eventValue.Clone();
                }
            }
        }
        finally
        {
            Native.EventsFree(subscription);
        }
    }
}
