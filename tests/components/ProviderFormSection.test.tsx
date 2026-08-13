import { render, screen } from "@testing-library/react";
import { Link2 } from "lucide-react";
import { describe, expect, it } from "vitest";
import { ProviderFormSection } from "@/components/providers/forms/ProviderFormSection";

describe("ProviderFormSection", () => {
  it("provides a labelled section with shared content and action slots", () => {
    render(
      <ProviderFormSection
        sectionKey="connection"
        icon={Link2}
        title="Connection"
        actions={<button type="button">Add</button>}
      >
        <label htmlFor="endpoint">Endpoint</label>
        <input id="endpoint" />
      </ProviderFormSection>,
    );

    const section = screen.getByRole("region", { name: "Connection" });
    expect(section).toHaveAttribute("data-provider-section", "connection");
    expect(screen.getByRole("button", { name: "Add" })).toBeInTheDocument();
    expect(screen.getByLabelText("Endpoint")).toBeInTheDocument();
  });
});
