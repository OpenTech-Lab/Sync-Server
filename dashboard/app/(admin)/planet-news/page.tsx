import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
      <Card className="py-0">
        <CardHeader>
          <CardTitle className="text-2xl">Planet News</CardTitle>
          <CardDescription>
            Publish markdown news posts for mobile users in the Planet tab.
          </CardDescription>
        </CardHeader>
      </Card>
      <PlanetNewsForm initialNews={news} />
    </div>
  );
}
