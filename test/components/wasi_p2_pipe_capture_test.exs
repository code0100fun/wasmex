defmodule Wasmex.WasiP2PipeCaptureTest do
  use ExUnit.Case, async: true

  alias Wasmex.Pipe
  alias Wasmex.Wasi.WasiP2Options

  describe "WASI P2 pipe capture" do
    test "captures stdout output" do
      {:ok, stdout} = Pipe.new()
      {:ok, stderr} = Pipe.new()

      component_bytes = File.read!(TestHelper.component_stdio_test_file_path())

      pid =
        start_supervised!(
          {Wasmex.Components,
           bytes: component_bytes,
           wasi: %WasiP2Options{
             stdout: stdout,
             stderr: stderr
           }}
        )

      # Call a function that writes to stdout
      assert {:ok, "printed to stdout"} =
               Wasmex.Components.call_function(pid, "print-hello", [])

      # Sync the captured output
      assert {:ok, {stdout_bytes, 0}} = Wasmex.Components.sync_pipe_output(pid)
      assert stdout_bytes > 0

      # Read from the pipe
      Pipe.seek(stdout, 0)
      output = Pipe.read(stdout)
      assert output =~ "Hello from stdout!"
    end

    test "captures stderr output" do
      {:ok, stdout} = Pipe.new()
      {:ok, stderr} = Pipe.new()

      component_bytes = File.read!(TestHelper.component_stdio_test_file_path())

      pid =
        start_supervised!(
          {Wasmex.Components,
           bytes: component_bytes,
           wasi: %WasiP2Options{
             stdout: stdout,
             stderr: stderr
           }}
        )

      # Call a function that writes to stderr
      assert {:ok, "printed to stderr"} =
               Wasmex.Components.call_function(pid, "print-error", [])

      # Sync the captured output
      assert {:ok, {0, stderr_bytes}} = Wasmex.Components.sync_pipe_output(pid)
      assert stderr_bytes > 0

      # Read from the pipe
      Pipe.seek(stderr, 0)
      output = Pipe.read(stderr)
      assert output =~ "Error from stderr!"
    end

    test "captures both stdout and stderr" do
      {:ok, stdout} = Pipe.new()
      {:ok, stderr} = Pipe.new()

      component_bytes = File.read!(TestHelper.component_stdio_test_file_path())

      pid =
        start_supervised!(
          {Wasmex.Components,
           bytes: component_bytes,
           wasi: %WasiP2Options{
             stdout: stdout,
             stderr: stderr
           }}
        )

      # Call a function that writes to both
      assert {:ok, "printed to both"} =
               Wasmex.Components.call_function(pid, "print-mixed", [])

      # Sync the captured output
      assert {:ok, {stdout_bytes, stderr_bytes}} = Wasmex.Components.sync_pipe_output(pid)
      assert stdout_bytes > 0
      assert stderr_bytes > 0

      # Read from stdout
      Pipe.seek(stdout, 0)
      stdout_output = Pipe.read(stdout)
      assert stdout_output =~ "This goes to stdout"
      assert stdout_output =~ "More stdout"

      # Read from stderr
      Pipe.seek(stderr, 0)
      stderr_output = Pipe.read(stderr)
      assert stderr_output =~ "This goes to stderr"
    end

    test "works with inherit mode" do
      component_bytes = File.read!(TestHelper.component_stdio_test_file_path())

      pid =
        start_supervised!(
          {Wasmex.Components,
           bytes: component_bytes,
           wasi: %WasiP2Options{
             inherit_stdout: true,
             inherit_stderr: true
           }}
        )

      # Should not crash with inherit mode
      # Note: Inherited output goes directly to OS file descriptors and can't be captured by capture_io
      assert {:ok, "printed to stdout"} =
               Wasmex.Components.call_function(pid, "print-hello", [])
    end

    test "mixed mode: capture stdout, inherit stderr" do
      {:ok, stdout} = Pipe.new()

      component_bytes = File.read!(TestHelper.component_stdio_test_file_path())

      pid =
        start_supervised!(
          {Wasmex.Components,
           bytes: component_bytes,
           wasi: %WasiP2Options{
             stdout: stdout,
             inherit_stderr: true
           }}
        )

      # Should work with mixed mode
      # Note: Inherited stderr goes directly to OS file descriptors and can't be captured by capture_io
      assert {:ok, "printed to both"} =
               Wasmex.Components.call_function(pid, "print-mixed", [])
    end

    test "default mode: neither pipe nor inherit" do
      component_bytes = File.read!(TestHelper.component_stdio_test_file_path())

      pid =
        start_supervised!({Wasmex.Components, bytes: component_bytes, wasi: %WasiP2Options{}})

      # Should work with output discarded
      assert {:ok, "printed to stdout"} =
               Wasmex.Components.call_function(pid, "print-hello", [])
    end
  end

  describe "backward compatibility" do
    test "works without any WASI options" do
      component_bytes = File.read!(TestHelper.component_stdio_test_file_path())

      pid =
        start_supervised!({Wasmex.Components, bytes: component_bytes})

      # Should still work without WASI options
      assert {:ok, "printed to stdout"} =
               Wasmex.Components.call_function(pid, "print-hello", [])
    end
  end
end
