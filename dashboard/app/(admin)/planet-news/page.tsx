import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { PlanetNewsForm } from "./ui/planet-news-form";

type ServerNewsItem = {
  id: string;
  title: string;
  summary: string | null;
  markdown_content: string;
  created_by: string | null;
  published_at: string;
  updated_at: string;
};

export default async function PlanetNewsPage() {
  await requireAdminSession();
  const news = await apiGetJson<ServerNewsItem[]>("/api/admin/server-news?limit=100");

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-xl font-semibold">Planet News</h1>
        <p className="text-sm text-muted-foreground">
          Publish markdown news posts for mobile users in the Planet tab.
        </p>
      </div>
      <PlanetNewsForm initialNews={news} />
    </div>
  );
}
