defmodule Wasmex.ComponentResourceTest do
  use ExUnit.Case, async: true

  alias Wasmex.Wasi.WasiP2Options

  defp start_component do
    component_bytes = File.read!(TestHelper.component_resource_test_file_path())

    start_supervised!(
      {Wasmex.Components, bytes: component_bytes, imports: %{}, wasi: %WasiP2Options{}}
    )
  end

  describe "resource types" do
    test "create and use Counter resource" do
      component_pid = start_component()

      # Create a Counter with initial value 10
      assert {:ok, counter_resource} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "create-counter"],
                 [10]
               )

      # Counter should be a resource reference (opaque Elixir resource)
      assert is_reference(counter_resource)

      # Use the counter by adding 5 to it
      assert {:ok, 15} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "use-counter"],
                 [
                   counter_resource,
                   5
                 ]
               )
    end

    test "multiple resources maintain independent state" do
      component_pid = start_component()

      # Create first counter with initial value 10
      assert {:ok, counter1} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "create-counter"],
                 [10]
               )

      # Create second counter with initial value 100
      assert {:ok, counter2} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "create-counter"],
                 [100]
               )

      # Use each counter once (owned resources are consumed after use)
      assert {:ok, 15} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "use-counter"],
                 [counter1, 5]
               )

      assert {:ok, 125} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "use-counter"],
                 [counter2, 25]
               )
    end

    test "resources can be created with different initial values" do
      component_pid = start_component()

      # Create multiple counters and verify they can be used with their initial values
      assert {:ok, counter1} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "create-counter"],
                 [0]
               )

      assert {:ok, counter2} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "create-counter"],
                 [50]
               )

      assert {:ok, counter3} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "create-counter"],
                 [200]
               )

      # Verify each counter works with its initial value
      assert {:ok, 10} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "use-counter"],
                 [counter1, 10]
               )

      assert {:ok, 75} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "use-counter"],
                 [counter2, 25]
               )

      assert {:ok, 250} =
               Wasmex.Components.call_function(
                 component_pid,
                 ["component:resource-test/types", "use-counter"],
                 [counter3, 50]
               )
    end
  end
end
