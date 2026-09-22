/* Interruptible critical damping. Position and velocity survive direction changes. */
(function(root){
'use strict';
class MotionState {
 constructor(value=0){this.value=value;this.target=value;this.velocity=0;this.time=null;this.active=false}
 snap(value,now=null){this.value=value;this.target=value;this.velocity=0;this.time=now;this.active=false;return value}
 retarget(target,now,duration=240,speed=1){
  if(this.active)this.sample(now,duration,speed);
  this.target=target;this.time=now;this.active=Math.abs(this.value-target)>1e-7||Math.abs(this.velocity)>1e-7;
 }
 sample(now,duration=240,speed=1){
  if(!this.active)return this.value;
  const dt=Math.max(0,(now-this.time)/1000);this.time=now;
  const w=9.25*1000*speed/duration,e=this.value-this.target,c=this.velocity+w*e,decay=Math.exp(-w*dt);
  this.value=this.target+(e+c*dt)*decay;this.velocity=(this.velocity-w*c*dt)*decay;
  if(this.value<0||this.value>1){this.value=Math.max(0,Math.min(1,this.value));this.velocity=0}
  if(Math.abs(this.value-this.target)<.001&&Math.abs(this.velocity)<.035)this.snap(this.target,now);
  return this.value;
 }
}
if(typeof module!=='undefined')module.exports=MotionState;else root.ZeronMotionState=MotionState;
})(typeof window!=='undefined'?window:globalThis);
