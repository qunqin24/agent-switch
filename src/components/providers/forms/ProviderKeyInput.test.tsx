import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ProviderKeyInput } from "./ProviderKeyInput";

function ControlledProviderKeyInput() {
  const [value, setValue] = useState("");
  return (
    <>
      <ProviderKeyInput value={value} onValueChange={setValue} />
      <output data-testid="provider-key-value">{value}</output>
    </>
  );
}

describe("ProviderKeyInput", () => {
  it("keeps Pinyin preedit text local until composition ends", () => {
    render(<ControlledProviderKeyInput />);
    const input = screen.getByRole("textbox");

    fireEvent.compositionStart(input);
    fireEvent.change(input, { target: { value: "qunqin" } });

    expect(input).toHaveValue("qunqin");
    expect(screen.getByTestId("provider-key-value")).toHaveTextContent("");

    fireEvent.compositionEnd(input);

    expect(input).toHaveValue("qunqin");
    expect(screen.getByTestId("provider-key-value")).toHaveTextContent(
      "qunqin",
    );
  });
});
