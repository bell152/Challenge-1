import { PortalClient } from "../../components/portal-client";

export default function MemberPortalPage() {
  return (
    <PortalClient
      expectedSubjectType="MEMBER"
      portalApiPath="/api/portal/member/home"
      title="Member Portal"
      description="这是 Member 对应的 portal 页面，当前展示主体信息、Member portal API 返回数据和会话列表。"
    />
  );
}
