import test from 'node:test';
import assert from 'node:assert/strict';
import {rootTasks,teamPhaseLabel,canReviewAccept} from './teams.ts';
import type {Task} from './model.ts';
test('workspace groups child tasks under parent and gates acceptance on current review',()=>{
 const parent={id:'parent',team:{phase:'review',cancelled:false,review:null}} as unknown as Task;
 const worker={id:'worker',parentLink:{parentId:'parent',role:'worker'}} as unknown as Task;
 assert.deepEqual(rootTasks([parent,worker]).map(t=>t.id),['parent']);assert.equal(teamPhaseLabel(parent),'独立评审');assert.equal(canReviewAccept(parent),false);
 parent.team!.phase='ready';parent.team!.review={packet:{verdict:'pass'}} as any;assert.equal(canReviewAccept(parent),true);parent.team!.cancelled=true;assert.equal(canReviewAccept(parent),false);
});
