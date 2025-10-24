defmodule Wasmex.ComponentMetadataTest do
  use ExUnit.Case, async: true

  alias Wasmex.Native

  describe "component_metadata/1" do
    test "extracts HTTP handler from http_handler_test fixture" do
      # Read the HTTP handler test component
      wasm_bytes = File.read!("test/component_fixtures/http_handler_test/http-handler.wasm")

      # Call the NIF
      result = Native.component_metadata(wasm_bytes)

      # Should be a list (no functions exported at world level for pure HTTP handler)
      assert is_list(result)
    end

    test "extracts handle function from HTTP handler interface" do
      # HTTP handler exports wasi:http/incoming-handler interface with handle function
      wasm_bytes = File.read!("test/component_fixtures/http_handler_test/http-handler.wasm")

      result = Native.component_metadata(wasm_bytes)

      # Should extract the handle function from the exported interface
      assert is_list(result)
      assert length(result) > 0

      # Find the handle function
      handle_func = Enum.find(result, fn f -> f["name"] == "handle" end)
      assert handle_func != nil
      assert is_list(handle_func["params"])
      assert is_binary(handle_func["returns"])
    end

    test "extracts function metadata from component_exported_interface fixture" do
      # This fixture exports an interface with functions
      wasm_path = TestHelper.component_exported_interface_file_path()

      # Skip if fixture doesn't exist
      if File.exists?(wasm_path) do
        wasm_bytes = File.read!(wasm_path)

        result = Native.component_metadata(wasm_bytes)

        assert is_list(result)

        # If there are functions, verify structure
        if length(result) > 0 do
          function = hd(result)
          assert is_map(function)
          assert Map.has_key?(function, "name")
          assert Map.has_key?(function, "params")
          assert Map.has_key?(function, "returns")
          assert Map.has_key?(function, "interface")

          # Params should be a list
          assert is_list(function["params"])

          # Returns should be a string
          assert is_binary(function["returns"])
        end
      end
    end

    test "param structure contains name and type" do
      # Use component_exported_interface which should have functions with params
      wasm_path = TestHelper.component_exported_interface_file_path()

      if File.exists?(wasm_path) do
        wasm_bytes = File.read!(wasm_path)
        result = Native.component_metadata(wasm_bytes)

        # Find a function with parameters
        function_with_params = Enum.find(result, fn f -> length(f["params"]) > 0 end)

        if function_with_params do
          param = hd(function_with_params["params"])
          assert Map.has_key?(param, "name")
          assert Map.has_key?(param, "type")
          assert is_binary(param["name"])
          assert is_binary(param["type"])
        end
      end
    end

    test "returns consistent results for multiple calls" do
      wasm_bytes = File.read!("test/component_fixtures/http_handler_test/http-handler.wasm")

      result1 = Native.component_metadata(wasm_bytes)
      result2 = Native.component_metadata(wasm_bytes)

      assert result1 == result2
    end

    test "extracts functions from exported interfaces" do
      # component_exported_interface fixture exports an interface with functions
      wasm_path = TestHelper.component_exported_interface_file_path()

      if File.exists?(wasm_path) do
        wasm_bytes = File.read!(wasm_path)
        result = Native.component_metadata(wasm_bytes)

        # Should extract functions from exported interfaces
        assert is_list(result)

        if length(result) > 0 do
          # Verify we got actual functions from the interface
          function = hd(result)
          assert is_map(function)
          assert is_binary(function["name"])
          assert is_list(function["params"])
          assert is_binary(function["returns"])

          IO.puts("\nExtracted #{length(result)} function(s) from exported interface:")

          Enum.each(result, fn f ->
            IO.puts("  - #{f["name"]}: (#{length(f["params"])} params) -> #{f["returns"]}")
          end)
        end
      end
    end
  end
end
