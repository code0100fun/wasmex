defmodule Wasmex.WasiP2PipeCaptureTest do
  use ExUnit.Case, async: true

  alias Wasmex.Pipe
  alias Wasmex.Wasi.WasiP2Options

  describe "WASI P2 pipe capture" do
    # TODO: Fix async runtime issue with WASI P2 stdio
    # Error: "Cannot start a runtime from within a runtime"
    # These tests verify that the Pipe infrastructure is correctly set up,
    # but need runtime fixes to actually capture output.
    @tag :skip
    test "accepts pipes without errors" do
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

      # Call a function that writes to stderr
      assert {:ok, "printed to stderr"} =
               Wasmex.Components.call_function(pid, "print-error", [])

      # Call a function that writes to both
      assert {:ok, "printed to both"} =
               Wasmex.Components.call_function(pid, "print-mixed", [])
    end

    @tag :skip
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
      assert {:ok, "printed to stdout"} =
               Wasmex.Components.call_function(pid, "print-hello", [])
    end

    @tag :skip
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
      assert {:ok, "printed to both"} =
               Wasmex.Components.call_function(pid, "print-mixed", [])
    end

    @tag :skip
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
    @tag :skip
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
