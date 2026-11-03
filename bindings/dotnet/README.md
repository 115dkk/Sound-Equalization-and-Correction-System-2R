Programs that use this package must be distributed under terms compatible with the GNU AGPL-3.0.

# Secs2R for .NET

`Secs2R` exposes the SECS 2R room-correction engine and its authenticated local API client to .NET. It accepts and returns the same JSON values as the desktop application.

## Install

```console
dotnet add package Secs2R
```

The package targets .NET Standard 2.1 and .NET 8. Native libraries are included for Windows x64, Linux x64, macOS x64, and macOS arm64.

## Embedded engine

```csharp
using Secs2R;
using System.Text.Json;

using var engine = new Engine("settings.json");
using var request = JsonDocument.Parse("{\"method\":\"bootstrap\"}");
JsonElement info = engine.Call(request.RootElement);
Console.WriteLine(info.GetProperty("data").GetProperty("version"));
```

Long-running requests return a job identifier. Call `WaitJob` for the terminal event, or open `Events` to receive every later job event. `Events` subscribes when it is called, so open it before starting the job it should watch.

```csharp
await using EventStream events = engine.Events(cancellationToken);
JsonElement started = engine.Call(calibrateRequest);
await foreach (JsonElement jobEvent in events)
{
    Console.WriteLine(jobEvent.GetRawText());
}
```

An `EventStream` can be enumerated once; open another one for a second reader. Each stream queues up to 1024 events. A reader that falls further behind loses the newest ones, and `EventStream.DroppedEvents` counts them; when it grows, ask for the job's state with `WaitJob`.

## Running application

Enable the local API in the SECS 2R application, then create a client. With no token arguments, `Client` reads the application's default token file.

```csharp
using var client = new Client();
JsonElement status = client.Status();
Console.WriteLine(status.GetProperty("app_name"));
```

Pass `token` directly or use `tokenPath` for another token file. `Client.Events` connects to the running application's event socket on the calling thread; `Client.EventsAsync` connects on the thread pool and can be cancelled, for a UI thread.

## Errors

Engine, client, transport, protocol, and binding errors throw `SecsException`. Its `Code` is stable for program logic; `Message`, `Detail`, and `Retryable` carry the error text and retry guidance.

## What is not included

Native audio-device measurement, GPU acceleration, desktop screens, and dialogs are not included. Project files, imported and mock measurements, calibration, MIMO, results, and exports are available through the embedded engine.

## Licence

`Secs2R` and its native engine are licensed under AGPL-3.0-only. If you modify the software and offer it to users over a network, you must offer those users the corresponding source. The SECS 2R desktop application remains GPL-3.0-only. Repository: <https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R>.
