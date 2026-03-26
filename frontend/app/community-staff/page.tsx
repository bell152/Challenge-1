import { PortalClient } from "../../components/portal-client";

export default function CommunityStaffPortalPage() {
  return (
    <PortalClient
      expectedSubjectType="COMMUNITY_STAFF"
      portalApiPath="/api/portal/community/home"
      title="Community Staff Portal"
      description="这是 Community Staff 对应的 portal 页面，当前展示主体信息、Community portal API 返回数据和会话列表。"
    />
  );
}
