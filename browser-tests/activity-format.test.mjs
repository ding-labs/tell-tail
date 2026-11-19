import {test} from 'node:test';
import assert from 'node:assert/strict';
import {duration,age,rangeForBin} from '../web/activity-format.mjs';
test('unknown durations remain unknown and proportional bins preserve boundaries',()=>{
  assert.equal(duration(null),'—');assert.equal(duration(0),'0 ms');assert.equal(duration(1234),'1.23 s');
  assert.equal(age(null,100),'not observed');assert.equal(age(1000,0),'just now');
  const bins={start_ms:0,end_ms:121};const ranges=Array.from({length:60},(_,i)=>rangeForBin(bins,i));
  assert.equal(ranges[0][0],0);assert.equal(ranges.at(-1)[1],121);
  for(let i=1;i<ranges.length;i++)assert.equal(ranges[i-1][1],ranges[i][0]);
});
