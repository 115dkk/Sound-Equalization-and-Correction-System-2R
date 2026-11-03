using System;
using System.Collections.Generic;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace Secs2R;

/// <summary>
/// One native subscription to the events of an <see cref="Engine"/> or a <see cref="Client"/>.
/// The subscription starts when the stream is created, so every event emitted after
/// <see cref="Engine.Events"/>, <see cref="Client.Events"/> or <see cref="Client.EventsAsync"/>
/// returns reaches it: open the stream before starting the job it should watch. The stream can be
/// enumerated once; the enumeration frees the subscription when it ends, and
/// <see cref="Dispose"/> frees a stream that is never enumerated.
/// </summary>
public sealed class EventStream : IAsyncEnumerable<JsonElement>, IAsyncDisposable, IDisposable
{
    private const ulong PollMilliseconds = 200;
    private readonly CancellationToken _cancellationToken;
    private long _subscription;
    private int _enumerated;
    private long _droppedEvents;

    internal EventStream(ulong subscription, CancellationToken cancellationToken)
    {
        _subscription = unchecked((long)subscription);
        _cancellationToken = cancellationToken;
    }

    /// <summary>
    /// Gets how many events the native queue of this stream dropped because they were read too
    /// slowly, as of its latest poll. A dropped event can be a job's terminal event, so a caller
    /// that sees this grow should ask for the job's state, for example with
    /// <see cref="Engine.WaitJob"/>.
    /// </summary>
    public ulong DroppedEvents => unchecked((ulong)Interlocked.Read(ref _droppedEvents));

    /// <summary>Polls the subscription until it closes or the enumeration is cancelled.</summary>
    /// <param name="cancellationToken">Stops polling and frees the native subscription.</param>
    /// <returns>An enumerator of event JSON values.</returns>
    /// <exception cref="InvalidOperationException">The stream was already enumerated.</exception>
    public IAsyncEnumerator<JsonElement> GetAsyncEnumerator(CancellationToken cancellationToken = default)
    {
        if (Interlocked.Exchange(ref _enumerated, 1) != 0)
        {
            throw new InvalidOperationException("An EventStream can be enumerated once; open another one for a second reader.");
        }
        return Enumerate(cancellationToken);
    }

    /// <summary>
    /// Frees the native subscription. An enumeration in progress ends at its next poll; one that was
    /// never started (an enumerator whose <c>MoveNextAsync</c> was never called) would otherwise keep
    /// the subscription forever.
    /// </summary>
    public void Dispose()
    {
        Free();
        GC.SuppressFinalize(this);
    }

    /// <summary>Frees the native subscription, as <see cref="Dispose"/> does.</summary>
    /// <returns>A completed task.</returns>
    public ValueTask DisposeAsync()
    {
        Dispose();
        return default;
    }

    // Opening a client subscription connects a WebSocket, which can take a while; EventsAsync runs
    // it on the thread pool so the caller's thread (a UI thread, say) does not wait. The native
    // call cannot be interrupted, so a cancellation stops waiting for it and frees whatever it
    // opens once it returns.
    internal static async Task<EventStream> OpenAsync(Func<ulong> subscribe, CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        Task<ulong> opening = Task.Run(subscribe, CancellationToken.None);
        if (token.CanBeCanceled)
        {
            var cancelled = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            using (token.Register(() => cancelled.TrySetResult(true)))
            {
                if (await Task.WhenAny(opening, cancelled.Task).ConfigureAwait(false) != opening)
                {
                    _ = opening.ContinueWith(
                        finished =>
                        {
                            if (finished.Status == TaskStatus.RanToCompletion)
                            {
                                Native.EventsFree(finished.Result);
                            }
                            else
                            {
                                _ = finished.Exception;
                            }
                        },
                        CancellationToken.None,
                        TaskContinuationOptions.ExecuteSynchronously,
                        TaskScheduler.Default);
                    throw new OperationCanceledException(token);
                }
            }
        }
        return new EventStream(await opening.ConfigureAwait(false), token);
    }

    private async IAsyncEnumerator<JsonElement> Enumerate(CancellationToken cancellationToken)
    {
        using CancellationTokenSource linked =
            CancellationTokenSource.CreateLinkedTokenSource(_cancellationToken, cancellationToken);
        CancellationToken token = linked.Token;
        ulong subscription = unchecked((ulong)Interlocked.Read(ref _subscription));
        try
        {
            if (subscription == 0)
            {
                throw new ObjectDisposedException(nameof(EventStream));
            }
            while (true)
            {
                token.ThrowIfCancellationRequested();
                JsonElement poll = default;
                bool polled = false;
                try
                {
                    poll = await Task.Run(
                        () => Native.EventsNext(subscription, PollMilliseconds),
                        CancellationToken.None).ConfigureAwait(false);
                    polled = true;
                }
                catch (SecsException) when (Interlocked.Read(ref _subscription) == 0)
                {
                    // Dispose freed the subscription during the poll: the stream ends.
                }
                if (!polled)
                {
                    yield break;
                }
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
            Free();
        }
    }

    private void Free()
    {
        long subscription = Interlocked.Exchange(ref _subscription, 0);
        if (subscription != 0)
        {
            Native.EventsFree(unchecked((ulong)subscription));
        }
    }

    /// <summary>Frees the native subscription if the stream was neither enumerated nor disposed.</summary>
    ~EventStream() => Free();
}
