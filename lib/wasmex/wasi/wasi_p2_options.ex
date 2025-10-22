defmodule Wasmex.Wasi.WasiP2Options do
  @moduledoc ~S"""
  Configures WASI P2 support for a Wasmex.Components.Store.

  WASI (WebAssembly System Interface) P2 provides system interface capabilities
  to WebAssembly components, allowing them to interact with the host system in a
  controlled manner.

  ## Options

    * `:stdin` - A `Wasmex.Pipe` to use as stdin. When provided, takes precedence
      over `:inherit_stdin`. Defaults to `nil`.

    * `:stdout` - A `Wasmex.Pipe` to use as stdout for capturing output. When provided,
      takes precedence over `:inherit_stdout`. Defaults to `nil`.

    * `:stderr` - A `Wasmex.Pipe` to use as stderr for capturing error output. When
      provided, takes precedence over `:inherit_stderr`. Defaults to `nil`.

    * `:inherit_stdin` - When `true` and no `:stdin` pipe is provided, allows the
      component to read from the parent process's standard input. Defaults to `false`.

    * `:inherit_stdout` - When `true` and no `:stdout` pipe is provided, allows the
      component to write to the parent process's standard output. Defaults to `false`.

    * `:inherit_stderr` - When `true` and no `:stderr` pipe is provided, allows the
      component to write to the parent process's standard error. Defaults to `false`.

    * `:allow_http` - When `true`, enables HTTP capabilities for the component.
      Defaults to `false`.

    * `:args` - List of command-line arguments to pass to the component.
      Defaults to `[]`.

    * `:env` - Map of environment variables to make available to the component.
      Defaults to `%{}`.

  ## Stdio Behavior

  The stdio streams (stdin, stdout, stderr) follow this priority:

  1. If a `Pipe` is provided (e.g., `stdin: pipe`), use that pipe
  2. Else if `inherit_*: true`, inherit from parent process
  3. Else discard (null sink)

  ## Examples

  ### Capturing stdout and stderr

      iex> {:ok, stdout} = Wasmex.Pipe.new()
      iex> {:ok, stderr} = Wasmex.Pipe.new()
      iex> wasi_opts = %Wasmex.Wasi.WasiP2Options{
      ...>   stdout: stdout,
      ...>   stderr: stderr
      ...> }
      iex> {:ok, pid} = Wasmex.Components.start_link(%{
      ...>   path: "my_component.wasm",
      ...>   wasi: wasi_opts
      ...> })
      iex> # ... call component functions ...
      iex> Wasmex.Pipe.seek(stdout, 0)
      iex> Wasmex.Pipe.read(stdout)
      "captured output"

  ### Inheriting from parent process

      iex> wasi_opts = %Wasmex.Wasi.WasiP2Options{
      ...>   inherit_stdout: true,
      ...>   inherit_stderr: true
      ...> }

  ### Mixed mode (capture stdout, inherit stderr)

      iex> {:ok, stdout} = Wasmex.Pipe.new()
      iex> wasi_opts = %Wasmex.Wasi.WasiP2Options{
      ...>   stdout: stdout,
      ...>   inherit_stderr: true
      ...> }

  """

  alias Wasmex.Pipe

  defstruct stdin: nil,
            stdout: nil,
            stderr: nil,
            inherit_stdin: false,
            inherit_stdout: false,
            inherit_stderr: false,
            allow_http: false,
            args: [],
            env: %{}

  @type t :: %__MODULE__{
          args: [String.t()],
          env: %{String.t() => String.t()},
          stdin: Pipe.t() | nil,
          stdout: Pipe.t() | nil,
          stderr: Pipe.t() | nil,
          inherit_stdin: boolean(),
          inherit_stdout: boolean(),
          inherit_stderr: boolean(),
          allow_http: boolean()
        }
end
