import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { SidebarMenuSkeleton } from "./sidebar";

function skeletonWidth(container: HTMLElement): string {
  const text = container.querySelector<HTMLElement>(
    '[data-sidebar="menu-skeleton-text"]',
  );
  return text?.style.getPropertyValue("--skeleton-width") ?? "";
}

describe("SidebarMenuSkeleton", () => {
  it("picks a width between 50% and 90% and keeps it across re-renders", () => {
    const { container, rerender } = render(<SidebarMenuSkeleton />);
    const width = skeletonWidth(container);

    expect(width).toMatch(/^\d+%$/);
    const percent = Number.parseInt(width, 10);
    expect(percent).toBeGreaterThanOrEqual(50);
    expect(percent).toBeLessThan(90);

    for (let i = 0; i < 5; i += 1) {
      rerender(<SidebarMenuSkeleton showIcon={i % 2 === 0} />);
      expect(skeletonWidth(container)).toBe(width);
    }
  });
});
