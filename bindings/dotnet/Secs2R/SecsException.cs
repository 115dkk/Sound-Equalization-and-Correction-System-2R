using System;

namespace Secs2R;

/// <summary>Describes a structured error returned by SECS 2R or its native binding.</summary>
public sealed class SecsException : Exception
{
    internal SecsException(string code, string message, string detail, bool retryable)
        : base(message)
    {
        Code = code;
        Detail = detail;
        Retryable = retryable;
    }

    /// <summary>Gets the stable machine-readable error code.</summary>
    public string Code { get; }

    /// <summary>Gets the diagnostic detail supplied by the engine, client, or binding.</summary>
    public string Detail { get; }

    /// <summary>Gets whether retrying the same operation can succeed.</summary>
    public bool Retryable { get; }
}
