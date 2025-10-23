defmodule Wasmex.Components.ProxyPre do
  @moduledoc """
  A pre-compiled WebAssembly component proxy for fast instantiation.

  ProxyPre allows efficient instantiation of HTTP handler components with fresh state
  per request, avoiding state pollution issues while maintaining low overhead.
  """

  @type t :: %__MODULE__{
          resource: binary(),
          reference: reference()
        }

  defstruct resource: nil,
            # The actual NIF ProxyPre resource.
            # Normally the compiler will happily do stuff like inlining the
            # resource in attributes. This will convert the resource into an
            # empty binary with no warning. This will make that harder to
            # accidentally do.
            reference: nil

  def __wrap_resource__(resource) do
    %__MODULE__{
      resource: resource,
      reference: make_ref()
    }
  end

  @doc """
  Creates a new ProxyPre from a store and component.

  This pre-compiles the component for fast instantiation later.

  ## Parameters
    * `store` - The store containing the engine and WASI context
    * `component` - The compiled WebAssembly component

  ## Returns
    * `{:ok, proxy_pre}` on success
    * `{:error, reason}` on failure
  """
  def new(store, component) do
    %{resource: store_resource} = store
    %{resource: component_resource} = component

    case Wasmex.Native.component_proxy_pre_new(store_resource, component_resource) do
      {:error, err} -> {:error, err}
      resource -> {:ok, __wrap_resource__(resource)}
    end
  end
end
