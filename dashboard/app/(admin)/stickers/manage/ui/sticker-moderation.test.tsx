import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { vi } from "vitest";

import { StickerModeration } from "./sticker-moderation";

const refreshMock = vi.fn();

vi.mock("next/navigation", () => ({
  useRouter: () => ({
    refresh: refreshMock,
  }),
}));

describe("StickerModeration", () => {
  it("posts approve action and refreshes", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({ ok: true, json: async () => ({ ok: true }) }),
    );

    render(
      <StickerModeration
        stickers={[
          {
            id: "sticker-1",
            uploader_id: "user-1",
            group_name: "Default",
            name: "smile",
            mime_type: "image/png",
            size_bytes: 16,
            status: "pending",
            created_at: new Date().toISOString(),
          },
        ]}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Approve" }));

    await waitFor(() => {
      expect(fetch).toHaveBeenCalled();
      expect(refreshMock).toHaveBeenCalled();
    });

    vi.unstubAllGlobals();
  });
});
