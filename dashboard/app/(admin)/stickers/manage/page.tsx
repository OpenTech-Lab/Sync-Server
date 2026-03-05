import { redirect } from "next/navigation";

export default async function StickerManagePage({
  searchParams,
}: {
  searchParams: Promise<{ group?: string }>;
}) {
  const { group } = await searchParams;
  if (group) {
    redirect(`/stickers?group=${encodeURIComponent(group)}`);
  }
  redirect("/stickers");
}
