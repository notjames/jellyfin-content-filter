using System.Net;
using System.Net.Http.Headers;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using Microsoft.Extensions.Logging;

namespace Jellyfin.Plugin.ContentFilter.Services;

/// <summary>Calls the local Rust daemon.</summary>
public interface IDaemonClient
{
    /// <summary>Sends a JSON request to the daemon.</summary>
    Task<T> SendAsync<T>(HttpMethod method, string path, object? body, CancellationToken cancellationToken);

    /// <summary>Sends a request where the response body is not required.</summary>
    Task SendAsync(HttpMethod method, string path, object? body, CancellationToken cancellationToken);
}

/// <summary>Raised when the local daemon cannot provide a response.</summary>
public sealed class DaemonUnavailableException : Exception
{
    /// <summary>Initializes a new instance of the <see cref="DaemonUnavailableException"/> class.</summary>
    public DaemonUnavailableException(string message, Exception? innerException = null)
        : base(message, innerException)
    {
    }
}

/// <summary>Raised when the daemon returns an HTTP error response.</summary>
public sealed class DaemonResponseException : Exception
{
    /// <summary>Initializes a new instance of the <see cref="DaemonResponseException"/> class.</summary>
    public DaemonResponseException(HttpStatusCode statusCode, string message)
        : base(message)
    {
        StatusCode = statusCode;
    }

    /// <summary>Gets the daemon HTTP status.</summary>
    public HttpStatusCode StatusCode { get; }
}

/// <summary>HTTP-over-Unix-domain-socket daemon client.</summary>
public sealed class DaemonClient : IDaemonClient, IDisposable
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private readonly HttpClient _httpClient;
    private readonly ILogger<DaemonClient> _logger;

    /// <summary>Initializes a new instance of the <see cref="DaemonClient"/> class.</summary>
    public DaemonClient(ILogger<DaemonClient> logger)
    {
        _logger = logger;
        var handler = new SocketsHttpHandler
        {
            ConnectCallback = ConnectAsync,
            PooledConnectionLifetime = TimeSpan.FromMinutes(1),
        };
        _httpClient = new HttpClient(handler)
        {
            BaseAddress = new Uri("http://localhost"),
            Timeout = Timeout.InfiniteTimeSpan,
        };
    }

    /// <inheritdoc />
    public async Task<T> SendAsync<T>(
        HttpMethod method,
        string path,
        object? body,
        CancellationToken cancellationToken)
    {
        var (payload, _) = await SendCoreAsync(method, path, body, cancellationToken).ConfigureAwait(false);
        return JsonSerializer.Deserialize<T>(payload, JsonOptions)
            ?? throw new DaemonUnavailableException("Daemon returned an empty JSON response.");
    }

    /// <inheritdoc />
    public async Task SendAsync(
        HttpMethod method,
        string path,
        object? body,
        CancellationToken cancellationToken)
        => _ = await SendCoreAsync(method, path, body, cancellationToken).ConfigureAwait(false);

    /// <inheritdoc />
    public void Dispose() => _httpClient.Dispose();

    private async Task<(string Payload, HttpStatusCode StatusCode)> SendCoreAsync(
        HttpMethod method,
        string path,
        object? body,
        CancellationToken cancellationToken)
    {
        var configuration = Plugin.Instance?.Configuration
            ?? throw new DaemonUnavailableException("Plugin configuration is unavailable.");
        using var request = new HttpRequestMessage(method, path);
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        if (!string.IsNullOrWhiteSpace(configuration.DaemonToken))
        {
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", configuration.DaemonToken);
        }

        if (body is not null)
        {
            request.Content = new StringContent(
                JsonSerializer.Serialize(body, JsonOptions),
                Encoding.UTF8,
                "application/json");
        }

        var timeout = TimeSpan.FromSeconds(Math.Clamp(configuration.TimeoutSeconds, 1, 120));
        using var timeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeoutSource.CancelAfter(timeout);
        try
        {
            using var response = await _httpClient.SendAsync(
                request,
                HttpCompletionOption.ResponseHeadersRead,
                timeoutSource.Token).ConfigureAwait(false);
            var payload = await response.Content.ReadAsStringAsync(timeoutSource.Token).ConfigureAwait(false);
            if (!response.IsSuccessStatusCode)
            {
                var detail = string.IsNullOrWhiteSpace(payload)
                    ? response.ReasonPhrase ?? "Unknown daemon error"
                    : payload;
                throw new DaemonResponseException(
                    response.StatusCode,
                    $"Daemon returned {(int)response.StatusCode}: {detail}");
            }

            return (payload, response.StatusCode);
        }
        catch (OperationCanceledException ex) when (!cancellationToken.IsCancellationRequested)
        {
            _logger.LogWarning("Content filter daemon request timed out after {Timeout}", timeout);
            throw new DaemonUnavailableException("Content filter daemon timed out.", ex);
        }
        catch (HttpRequestException ex)
        {
            _logger.LogWarning(ex, "Content filter daemon request failed");
            throw new DaemonUnavailableException("Content filter daemon is unavailable.", ex);
        }
    }

    private static async ValueTask<Stream> ConnectAsync(
        SocketsHttpConnectionContext context,
        CancellationToken cancellationToken)
    {
        var socketPath = Plugin.Instance?.Configuration.SocketPath;
        if (string.IsNullOrWhiteSpace(socketPath) || !Path.IsPathFullyQualified(socketPath))
        {
            throw new InvalidOperationException("The daemon socket path must be absolute.");
        }

        var socket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        try
        {
            await socket.ConnectAsync(new UnixDomainSocketEndPoint(socketPath), cancellationToken)
                .ConfigureAwait(false);
            return new NetworkStream(socket, ownsSocket: true);
        }
        catch
        {
            socket.Dispose();
            throw;
        }
    }
}
