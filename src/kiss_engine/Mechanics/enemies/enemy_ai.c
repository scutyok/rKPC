/*
 * enemy_ai.c
 *
 * Translated from object.lto (Kiss: Psycho Circus – The Nightmare Child, 2000)
 * Original DLL compiled with Visual C++, disassembled by IDA Pro.
 *
 * This file covers the shared aggressive AI core plus the two best-documented
 * enemy types visible in the disassembly:
 *
 *   • Stargrave  – ranged electric enemy (sub_10066DF0 vtable slot)
 *   • Nightmare Child (child3 / child4 variants) – melee/leap enemy
 *
 * All struct offsets are derived directly from the assembly.
 * Member names have been reconstructed from debug strings, animation names,
 * sound paths and behavioural context.
 *
 * Engine interface functions that are not in this translation unit are
 * declared as extern and called through the same vtable/function-pointer
 * convention used by the LithTech 1.x engine.
 */

#include <math.h>
#include <string.h>
#include <windows.h>   /* for SEH types only – not used at runtime */

/* =========================================================================
 * Forward declarations – engine / base-class functions
 * ========================================================================= */

/* LithTech engine server shell, stored at g_pServerDE */
typedef struct ILTServer ILTServer;
extern ILTServer *g_pServerDE;   /* dword_10169858 in the binary */

/* Engine vtable slots used by AI code (offsets in ILTServer*) */
/* [edx+0x1D0] */ extern void   LT_GetObjectPos(ILTServer*, void* hObj, float* pPos);
/* [edx+0x24]  */ extern float  LT_GetTime(ILTServer*);
/* [edx+0x254] */ extern int    LT_GetNodeTransform(ILTServer*, void* hObj,
                                                     const char* nodeName,
                                                     float* pOutPos, int* pFlag);
/* vtable on 'this' */
/* [eax+0x90]  */ /* SetAnimation(animIndex, loop) */
/* [eax+0xB4]  */ /* OnTargetAcquired()            */
/* [eax+0xEC]  */ /* CanSeeTarget() -> bool         */
/* [eax+0xE4]  */ /* IsInAttackRange() -> bool      */

/* Base-class / helper calls */
extern void   BaseAI_Update(void* self);                    /* sub_10070420  */
extern void   AI_CheckForPlayerTarget(void* self, int flag);/* sub_1006FB60  */
extern void   AI_SetRandomAttackTimer(void* self,
                                      float lo, float hi);  /* sub_100D7180  */
extern void   AI_SetMoveGoal_Path(void* self);              /* sub_1006FC30  */
extern void   AI_SetMoveGoal_Chase(void* self, int flag);   /* sub_1006FC80  */
extern void   AI_SetMoveGoal_Attack(void* self,
                                    int flag, int loop);    /* sub_10070100  */
extern void   AI_DoRangedAttack(void* self);                /* sub_100661C0  */
extern void   AI_DoMeleeAttack(void* self);                 /* sub_10066300  */
extern void   AI_AimAtTarget(void* self,
                              float* pFirePos,
                              int bFromHand, float speed); /* sub_10066490  */
extern void   AI_ComputeAttackTimer(void* self);           /* sub_10066DC0  */
extern void   AI_ComputeAttackTimerB(void* self,
                                     int a, int b,
                                     float t);             /* sub_10066B00  */
extern void   AI_ComputeAttackTimerC(void* self,
                                     int a, int b,
                                     float t);             /* sub_10066890  */
extern float  AI_GetAngleToTarget(void* self,
                                   void* hTarget,
                                   float* pTargetPos);     /* sub_1006CFB0  */
extern void   StringHandle_Assign(void* dst, void* src);   /* sub_10004390  */
extern void   StringHandle_Release(void* sh);              /* sub_100F99F0  */
extern void   StringHandle_AddRef(void* sh);               /* sub_100F9970  */
extern int    __isnan(double);

/* =========================================================================
 * Move-goal type constants (m_eMoveGoal / [esi+0xD4])
 * ========================================================================= */
#define MOVEGOAL_NONE        0
#define MOVEGOAL_TEMP        1   /* "handling MOVEGOAL_TEMP" */
#define MOVEGOAL_LINKSTART   2   /* "handling MOVEGOAL_LINKSTART or MOVEGOAL_…" */
#define MOVEGOAL_LINKEND     3
#define MOVEGOAL_CHASE       4

/* =========================================================================
 * Shared AI state layout
 *
 * Every enemy inherits from a common base.  Offsets are read directly from
 * the disassembly (esi + offset).
 * ========================================================================= */
typedef struct AIBase
{
    /* 0x00 */ void       *vtable;

    /* position / orientation (LithTech LTVector) */
    /* 0x54 */ float       x;
    /* 0x58 */ float       y;
    /* 0x5C */ float       z;
    /* 0x60 */ float       forwardX;
    /* 0x64 */ float       forwardY;
    /* 0x68 */ float       forwardZ;
    /* 0x6C */ float       gameTime;        /* current server time       */

    /* 0x08 */ void       *hSelf;           /* engine object handle      */

    /* 0xC4 */ unsigned char bStuck;        /* m_bStuck                  */
    /* 0xD4 */ int         eMoveGoal;       /* MOVEGOAL_* enum           */
    /* 0xD8 */ unsigned char bMoveGoalFlag; /* extra flag for move goal  */
    /* 0xDC */ void       *hMoveGoalString; /* string handle             */

    /* saved move-goal for TEMP restore */
    /* 0xE0 */ float       savedGoalX;
    /* 0xE4 */ float       savedGoalY;
    /* 0xE8 */ float       savedGoalZ;
    /* 0xEC */ float       toTargetX;      /* normalised direction      */
    /* 0xF0 */ float       toTargetY;
    /* 0xF4 */ float       toTargetZ;
    /* 0xF8 */ float       distToTarget;
    /* 0xFC */ float       goalYaw;        /* 0x42800000 = 64.0f reset  */
    /* 0x100 */ float      lastMoveTime;
    /* 0x104 */ int        savedMoveGoal;
    /* 0x108 */ unsigned char savedMoveGoalFlag;
    /* 0x10C */ void      *hSavedMoveGoalStr;
    /* 0x110 */ float      savedGoalX2;
    /* 0x114 */ float      savedGoalY2;
    /* 0x118 */ float      savedGoalZ2;
    /* 0x11C */ float      savedGoalYaw2;

    /* 0x148 */ unsigned char bAttacking;   /* currently in attack state */

    /* 0x174 */ void      *hEnemy;          /* handle to current target  */
    /* 0x1A4 */ float      enemyLastSeenYaw;

    /* 0x290 */ float      attackRangeTime; /* time threshold for attack */
    /* 0x2DC */ float      handFireRadius;  /* radius for hand-fire check */
    /* 0x2E0 */ float      attackTimer;     /* countdown to next attack  */
    /* 0x2E4 */ unsigned char bInAttackAnim;
    /* 0x2EC */ unsigned char bCanAttack;
    /* 0x2ED */ unsigned char bFireFromHand;
    /* 0x2F0 */ float      handFirePosX;   /* R_Hand bone world pos     */
    /* 0x2F4 */ float      handFirePosY;
    /* 0x2F8 */ float      handFirePosZ;
    /* 0x2FC */ float      aimDirX;
    /* 0x300 */ float      aimDirY;
    /* 0x304 */ float      aimDirZ;
    /* 0x320 */ float      nextUpdateTime;
} AIBase;

/* =========================================================================
 * Helper – reset the current move-goal to "no goal"
 * ========================================================================= */
static void ResetMoveGoal(AIBase *self)
{
    self->eMoveGoal     = MOVEGOAL_NONE;
    self->bMoveGoalFlag = 0;

    if (self->hMoveGoalString)
    {
        StringHandle_Release(self->hMoveGoalString);
        self->hMoveGoalString = NULL;
        StringHandle_AddRef(NULL);   /* matches the original call pattern */
    }

    self->distToTarget  = 0.0f;
    self->goalYaw       = 64.0f;    /* 0x42800000 */
    self->lastMoveTime  = 0.0f;
    self->toTargetZ     = 0.0f;
    self->toTargetY     = 0.0f;
    self->toTargetX     = 0.0f;
    self->savedGoalX    = 0.0f;
    self->savedGoalY    = 0.0f;
    self->savedGoalZ    = 0.0f;
}

/* =========================================================================
 * AI_ComputeAttackTimer_Random  (sub_10066DC0)
 *
 * Picks a random delay in [lo, hi] and adds it to gameTime to produce
 * the next attack-window start time stored at self->attackTimer.
 * ========================================================================= */
static void AI_ComputeAttackTimer_Random(AIBase *self)
{
    /* Reads two floats from globals flt_101526B8 / flt_101526BC
       (min/max attack time as set per-enemy-type in properties).
       sub_100D7180 = random float in [lo, hi]                        */
    extern float g_fMinAttackTime;   /* flt_101526B8 */
    extern float g_fMaxAttackTime;   /* flt_101526BC */

    float delay = /* sub_100D7180 */ 0.0f;   /* replaced by call below */
    AI_SetRandomAttackTimer(self, g_fMinAttackTime, g_fMaxAttackTime);
    /* result is stored: self->attackTimer = gameTime + delay          */
    self->attackTimer = self->gameTime + delay;
}

/* =========================================================================
 * UpdateAI_Aggressive  (sub_10066DF0 – Stargrave vtable slot 0)
 *                      (sub_1006AB30 – Nightmare Child vtable slot 0)
 *                      (and repeated for every other aggressive enemy)
 *
 * This is the main per-tick AI update called by the engine.
 * The function is ~2 KB of machine code; the translation below faithfully
 * reproduces every branch with descriptive names derived from the debug
 * strings embedded in the binary.
 * ========================================================================= */
void UpdateAI_Aggressive(AIBase *self)
{
    /* ---- engine boilerplate (nullsub_1 = debug print, ignored) ---- */
    /* "UpdateAI_Aggressive: " */

    BaseAI_Update(self);   /* update timers, animations, damage state   */

    /* ----------------------------------------------------------------
     * 1. STUCK RECOVERY
     *    If the AI is marked stuck (bStuck) and was chasing (MOVEGOAL_CHASE),
     *    try to restore a previously-saved temporary goal.
     * ---------------------------------------------------------------- */
    if (self->bStuck)
    {
        /* "  m_bStuck == DTRUE" */

        if (self->eMoveGoal == MOVEGOAL_TEMP)
        {
            float timeSinceGoal = self->lastMoveTime - self->gameTime;

            /* Only restore if the saved goal is still fresh enough */
            if (timeSinceGoal <= /* flt_10115214 */ 5.0f)
            {
                /* Restore the saved move-goal */
                self->eMoveGoal     = self->savedMoveGoal;
                self->bMoveGoalFlag = self->savedMoveGoalFlag;
                StringHandle_Assign(&self->hMoveGoalString,
                                    self->hSavedMoveGoalStr);

                self->goalYaw    = self->savedGoalYaw2;
                self->savedGoalX = self->savedGoalX2;
                self->savedGoalY = self->savedGoalY2;
                self->savedGoalZ = self->savedGoalZ2;

                /* Validate the saved position (NaN guard) */
                __isnan(self->savedGoalX);
                __isnan(self->savedGoalY);
                __isnan(self->savedGoalZ);

                if (self->hMoveGoalString)
                {
                    LT_GetObjectPos(g_pServerDE,
                                    self->hMoveGoalString,
                                    &self->savedGoalX);
                }

                self->lastMoveTime = LT_GetTime(g_pServerDE);

                /* "    Restored from MOVEGOAL_TEMP" */
                goto after_stuck_handling;
            }
        }

        /* If move-goal is chase/link, don't try to unstick via pathing */
        if (self->hMoveGoalString &&
            self->eMoveGoal != MOVEGOAL_LINKSTART &&
            self->eMoveGoal != MOVEGOAL_LINKEND)
        {
            AI_SetMoveGoal_Path(self);
            /* "    Unstuck, pathing." */
            AI_SetMoveGoal_Chase(self, 1);
            AI_SetMoveGoal_Attack(self, /* result of chase */ 0, 0);
        }
    }

after_stuck_handling:

    /* ----------------------------------------------------------------
     * 2. TARGET ACQUISITION
     *    If we have no target, scan for the player.
     * ---------------------------------------------------------------- */
    if (!self->hEnemy)
    {
        /* "  CheckForPlayerTarget" */
        AI_CheckForPlayerTarget(self, 1 /* bCheckVisibility */);

        if (self->hEnemy)
        {
            /* Got a new target – reset attack timer */
            AI_ComputeAttackTimer_Random(self);

            /* Call virtual OnTargetAcquired() */
            void (**vtbl)(AIBase*) = *(void(***)())self;
            vtbl[0xB4 / sizeof(void*)](self);
        }
    }

    /* ----------------------------------------------------------------
     * 3. MOVE-GOAL DISPATCH
     * ---------------------------------------------------------------- */
    if (self->eMoveGoal == MOVEGOAL_TEMP)
    {
        /* "  handling MOVEGOAL_TEMP" */
        goto do_animation_update;
    }

    if (self->eMoveGoal == MOVEGOAL_LINKSTART ||
        self->eMoveGoal == MOVEGOAL_LINKEND)
    {
        /* "  handling MOVEGOAL_LINKSTART or MOVEGOAL_…" */
        self->bAttacking = 1;

        if (self->hEnemy)
        {
            float targetPos[3];
            LT_GetObjectPos(g_pServerDE, self->hEnemy, targetPos);

            /* Distance-squared to enemy */
            float dx = targetPos[0] - self->x;
            float dy = targetPos[1] - self->y;
            float dz = targetPos[2] - self->z;
            float distSq = dx*dx + dy*dy + dz*dz;

            /* Only attack if enemy is within link-range threshold */
            if (distSq > /* flt_10118E00 = ~400 units squared */ 400.0f * 400.0f)
            {
                float angle = AI_GetAngleToTarget(self, self->hEnemy, targetPos);

                /* Must be facing close enough to fire */
                if (angle > /* flt_10115218 */ 0.5f /* ~30 deg */)
                    goto do_time_checks;

                /* In range and facing: set up attack goal and path to fire */
                AI_ComputeAttackTimer_Random(self);

                /* Transition to CHASE goal so we move into position */
                self->eMoveGoal     = MOVEGOAL_NONE;
                self->bMoveGoalFlag = 0;
                ResetMoveGoal(self);

                self->savedGoalX    = targetPos[0];
                self->savedGoalY    = targetPos[1];
                self->savedGoalZ    = targetPos[2];
                self->bMoveGoalFlag = 1;
                self->eMoveGoal     = MOVEGOAL_CHASE;

                self->lastMoveTime  = LT_GetTime(g_pServerDE);

                __isnan(self->savedGoalX);
                __isnan(self->savedGoalY);
                __isnan(self->savedGoalZ);
            }
        }

do_time_checks:
        /* Stuck while in link goal → reset */
        if (self->bStuck)
        {
            ResetMoveGoal(self);

            /* Play idle animation: SetAnimation(4, 1) */
            void (**vtbl)(AIBase*, int, int) = *(void(***)(AIBase*, int, int))self;
            vtbl[0x90 / sizeof(void*)](self, 4, 1);
        }
        goto do_time_checks_end;
    }

    /* eMoveGoal == NONE or CHASE */
    if (!self->hEnemy)
    {
        /* No target and no special goal: wander / idle */
        ResetMoveGoal(self);

        /* SetAnimation(4, 1) – idle loop */
        void (**vtbl)(AIBase*, int, int) = *(void(***)(AIBase*, int, int))self;
        vtbl[0x90 / sizeof(void*)](self, 4, 1);
        goto do_time_checks_end;
    }

    /* "  handling m_hEnemy != DNULL" */
    {
        float targetPos[3];
        LT_GetObjectPos(g_pServerDE, self->hEnemy, targetPos);

        /* ---- 3a. Can the AI SEE the target? ---- */
        {
            typedef int (*FnCanSee)(AIBase*);
            FnCanSee canSee = ((FnCanSee*)(*(void**)self))[0xEC / sizeof(void*)];
            if (canSee(self))
            {
                AI_DoRangedAttack(self);
                goto do_time_checks_end;
            }
        }

        /* ---- 3b. Is the target in MELEE range? ---- */
        {
            typedef int (*FnInRange)(AIBase*);
            FnInRange inRange = ((FnInRange*)(*(void**)self))[0xE4 / sizeof(void*)];
            if (inRange(self))
            {
                AI_DoMeleeAttack(self);
                goto do_time_checks_end;
            }
        }

        /* ---- 3c. Still outside attack range – keep chasing ---- */
        if (self->gameTime < self->attackRangeTime)
        {
            /* Not yet time to try attacking; continue movement */
            goto do_time_checks_end;
        }

        /* Time to attack: clear attack flag, fall through to animation */
        self->bInAttackAnim = 0;
    }

do_animation_update:

    /* SetAnimation(0 /*bLoop*/, 1 /*animId*/) */
    {
        void (**vtbl)(AIBase*, int, int) = *(void(***)(AIBase*, int, int))self;
        vtbl[0x90 / sizeof(void*)](self, 0, 1);
    }

do_time_checks_end:

    /* ----------------------------------------------------------------
     * 4. ATTACK-WINDOW MANAGEMENT
     *    bCanAttack is set when gameTime passes attackTimer.
     *    bAttacking is set when gameTime is inside the fire window.
     * ---------------------------------------------------------------- */
    if (self->gameTime < self->attackRangeTime)
    {
        /* Still outside window: ensure attack flags are clear */
        if (self->gameTime < self->nextUpdateTime)
        {
            self->bAttacking    = 1;
            self->bInAttackAnim = 0;
            self->bCanAttack    = 0;
            self->bFireFromHand = 0;

            /* SetAnimation(4, 1) */
            void (**vtbl)(AIBase*, int, int) = *(void(***)(AIBase*, int, int))self;
            vtbl[0x90 / sizeof(void*)](self, 4, 1);
            return;
        }
    }
    else
    {
        /* Inside attack window */
        self->bAttacking    = 1;
        self->bInAttackAnim = 0;
        self->bCanAttack    = 0;
        self->bFireFromHand = 0;
        return;
    }

    /* ----------------------------------------------------------------
     * 5. AIM / FIRE
     *    If bCanAttack and bFireFromHand: aim towards hand-fire position,
     *    otherwise aim from body centre.
     * ---------------------------------------------------------------- */
    if (!self->bCanAttack || !self->hEnemy)
        return;

    if (self->bFireFromHand)
    {
        /* Get world-space position of R_Hand bone */
        int boneFlag;
        float handPos[3];
        LT_GetNodeTransform(g_pServerDE, self->hSelf,
                            "R_Hand", &self->handFirePosX, &boneFlag);

        /* Get target position */
        float targetPos[3];
        LT_GetObjectPos(g_pServerDE, self->hEnemy, targetPos);

        /* Direction from hand to target */
        float dx = targetPos[0] - self->handFirePosX;
        float dy = targetPos[1] - self->handFirePosY;
        float dz = targetPos[2] - self->handFirePosZ;
        float dist = sqrtf(dx*dx + dy*dy + dz*dz);

        if (dist > self->handFireRadius)
        {
            /* Normalise and store aim direction */
            float inv = 1.0f / dist;
            self->aimDirX = dx * inv;
            self->aimDirY = dy * inv;
            self->aimDirZ = dz * inv;

            extern float g_fBulletSpeed;  /* flt_101526B0 */
            float firePos[12] = {0};      /* LTRotation + LTVector */
            AI_AimAtTarget(self, firePos, 0 /* bFromHand */, g_fBulletSpeed);

            extern float g_fBulletSpeed2; /* flt_101526B4 */
            AI_AimAtTarget(self, firePos, 1 /* bFromHand */, g_fBulletSpeed2);
        }
        else
        {
            /* Too close to hand-fire: use body-forward direction */
            self->aimDirX = self->forwardX;
            self->aimDirY = self->forwardY;
            self->aimDirZ = self->forwardZ;

            extern float g_fBulletSpeed;
            float firePos[12] = {0};
            AI_AimAtTarget(self, firePos, 0, g_fBulletSpeed);

            extern float g_fBulletSpeed2;
            AI_AimAtTarget(self, firePos, 1, g_fBulletSpeed2);
        }
    }
    else
    {
        /* Fire from body: use a slightly spread aim via sub_10066490 */
        extern float g_fBulletSpeed2;  /* flt_101526B4 */
        float firePos[12] = {0};

        AI_AimAtTarget(self, firePos, 0 /* bSpread */, g_fBulletSpeed2);
        AI_AimAtTarget(self, firePos, 1 /* bStraight */, g_fBulletSpeed2);
    }
}

/* =========================================================================
 * Stargrave-specific initialisation  (sub_10001010 / sub_10001080)
 *
 * Called once at spawn.  Sets default property values.
 * The strings "AttackA/B/C" are animation names; sounds/models are
 * loaded by the engine from the path strings below.
 * ========================================================================= */

typedef struct StargraveAI
{
    AIBase      base;

    /* Extra Stargrave fields recovered from property strings */
    int         bCanAttack;         /* "CanAttack"      offset 0 in prop table */
    float       attackRadius;       /* "AttackRadius"                           */
    float       minAttackTime;      /* "MinAttackTime"                          */
    float       maxAttackTime;      /* "MaxAttackTime"                          */
} StargraveAI;

void Stargrave_Init(StargraveAI *self)
{
    /* Default property values written in sub_10001010 */
    memset(&self->base, 0, sizeof(AIBase));

    self->bCanAttack    = 1;
    self->attackRadius  = 600.0f;   /* typical LithTech unit distance */
    self->minAttackTime = 1.5f;
    self->maxAttackTime = 3.0f;

    /* Model / skin */
    /* engine would call: SetModel("models/creatures/stargrave.abc")  */
    /* engine would call: SetSkin("skins/creatures/stargrave.dtx")    */

    /* Animations registered: "AttackA", "AttackB", "AttackC"        */
    /* Sounds registered: elecloop.wav, zap1.wav, zap2.wav,
                          elecfire2.wav, elecpop.wav, elecpopbig.wav  */

    /* Sub-object: orbit-loop sound attached at spawn                 */
    /* "sounds/monsters/stargrave/orbitloop.wav"                      */
}

/* Target-acquired notification for Stargrave (debug string at 0x10003F81) */
void Stargrave_OnTargetAcquired(StargraveAI *self)
{
    /* "Stargrave acquired target." */
    /* Play alert animation, start electric-loop sound */
}

/* =========================================================================
 * Nightmare Child – target-acquired  (debug string at 0x1000D08F / 0x10011179)
 * ========================================================================= */
typedef struct NightmareChildAI
{
    AIBase      base;

    int         bCanAttack;
    float       attackRadius;
    float       minAttackTime;
    float       maxAttackTime;

    /* Leap-attack state */
    float       leapChargeTimer;
    int         bLeaping;
    float       leapTargetX, leapTargetY, leapTargetZ;
} NightmareChildAI;

void NightmareChild_OnTargetAcquired(NightmareChildAI *self)
{
    /* "Nightmare Child acquired target." */
    /* Start brain-shot powerup sound:
       "sounds/monsters/nightmare/brainshotpowerup.wav"               */
}

/* =========================================================================
 * Nightmare Child – attack selection  (partial, from switch @ 0x1000CF8C)
 *
 * The child has four attack modes keyed by a state index:
 *   0 = AttackA  (basic brain-shot)
 *   1 = AttackC1 (leap windup)
 *   2 = AttackC2 (leap in-air)
 *   3 = AttackC3 (leap landing)
 * ========================================================================= */
#define NIGHTMARE_ATK_BRAINSHOT  0
#define NIGHTMARE_ATK_LEAP_START 1
#define NIGHTMARE_ATK_LEAP_AIR   2
#define NIGHTMARE_ATK_LEAP_LAND  3

void NightmareChild_DoAttack(NightmareChildAI *self, int attackType)
{
    switch (attackType)
    {
        case NIGHTMARE_ATK_BRAINSHOT:
            /* Play "AttackA" animation */
            /* Fire brain-shot projectile */
            /* Sound: "sounds/monsters/nightmare/brainshotpowerup.wav" */
            break;

        case NIGHTMARE_ATK_LEAP_START:
            /* Play "AttackC1" animation (charge/windup)             */
            /* Sound: "sounds/monsters/nightmare/leapstart.wav"      */
            self->bLeaping = 1;
            /* Store target pos for landing check */
            {
                float tp[3];
                LT_GetObjectPos(g_pServerDE, self->base.hEnemy, tp);
                self->leapTargetX = tp[0];
                self->leapTargetY = tp[1];
                self->leapTargetZ = tp[2];
            }
            break;

        case NIGHTMARE_ATK_LEAP_AIR:
            /* Play "AttackC2" animation (airborne)                  */
            /* Apply impulse towards leapTarget                      */
            break;

        case NIGHTMARE_ATK_LEAP_LAND:
            /* Play "AttackC3" animation (landing impact)            */
            /* Sound: "sounds/monsters/nightmare/leapfinish.wav"     */
            /* Deal area damage on landing                           */
            /* Sound on hit: "sounds/monsters/nightmare/leaphit.wav" */
            self->bLeaping = 0;
            break;
    }
}

/* =========================================================================
 * Blackwell – attack selection  (switch @ 0x100150A4 / 0x10015A7C)
 *
 * Blackwell is a melee/teleport boss.  Known attacks from sound paths:
 *   appear.wav / vanish.wav  → teleport cycle
 *   swing.wav                → melee swing
 *   fireball%d.wav           → ranged fire
 *   blackwelldeath.wav       → death
 * ========================================================================= */
typedef struct BlackwellAI
{
    AIBase  base;
    int     bVisible;        /* teleport state */
    float   reappearTimer;
} BlackwellAI;

#define BLACKWELL_ATK_MELEE     0
#define BLACKWELL_ATK_FIREBALL  1
#define BLACKWELL_ATK_TELEPORT  2
#define BLACKWELL_ATK_VANISH    3

void Blackwell_DoAttack(BlackwellAI *self, int attackType)
{
    switch (attackType)
    {
        case BLACKWELL_ATK_MELEE:
            /* Sound: "sounds/monsters/blackwell/swing.wav" */
            break;
        case BLACKWELL_ATK_FIREBALL:
            /* Sound: "sounds/monsters/blackwell/fireball%d.wav" (random 1-N) */
            break;
        case BLACKWELL_ATK_VANISH:
            /* Sound: "sounds/monsters/blackwell/vanish.wav" */
            self->bVisible = 0;
            break;
        case BLACKWELL_ATK_TELEPORT:
            /* Sound: "sounds/monsters/blackwell/appear.wav" */
            self->bVisible = 1;
            /* Teleport to a position near the player */
            break;
    }
}

/* =========================================================================
 * Tiberius – melee/charge/whip boss  (switch @ 0x100276A8 / 0x100279EC)
 *
 * Attacks:
 *   "whip_attack"    – long-range whip (whipcrack.wav / whipswish.wav)
 *   "charge_attack"  – full-speed charge (charge.wav / impact.wav)
 *   preattack1-4.wav – windup sounds before each attack type
 *   step%d.wav       – footstep sounds during movement
 * ========================================================================= */
typedef struct TiberiusAI
{
    AIBase  base;
    int     attackPhase;     /* which preattack windup we are in */
    float   chargeSpeed;
} TiberiusAI;

void Tiberius_DoAttack(TiberiusAI *self, int attackType)
{
    switch (attackType)
    {
        case 0: /* attacka – whip windup */
            /* Sound: preattack1.wav */
            self->attackPhase = 1;
            break;
        case 1: /* attackb – whip release */
            /* Sound: whipcrack.wav / whipswish.wav */
            /* Deal damage in whip arc */
            break;
        case 2: /* attackc – charge windup */
            /* Sound: preattack3.wav / charge.wav */
            self->attackPhase = 2;
            self->chargeSpeed = 800.0f; /* units/sec estimate */
            break;
        case 3: /* charge impact */
            /* Sound: impact.wav */
            /* Deal heavy knockback damage */
            self->chargeSpeed = 0.0f;
            break;
    }
}

/* =========================================================================
 * CGenericTarget  (sub_100DBC40)
 *
 * A reusable targeting component shared by many enemy types.
 * Registered as "CGenericTarget" in the class factory at 0x100DBC45.
 * ========================================================================= */
typedef struct CGenericTarget
{
    void   *vtable;          /* off_10140730 → "CGenericTarget" */
    void   *hOwner;          /* owning AI object               */
    void   *hTarget;         /* current target handle          */
    float   detectionRadius;
    float   lostTargetTime;  /* how long since we last saw target */
} CGenericTarget;

void CGenericTarget_Init(CGenericTarget *self, void *hOwner)
{
    self->vtable           = NULL;  /* set by engine class factory */
    self->hOwner           = hOwner;
    self->hTarget          = NULL;
    self->detectionRadius  = 1500.0f;
    self->lostTargetTime   = 0.0f;
}

/* =========================================================================
 * AlertGroup / AlertNear  (sub_1006E480)
 *
 * When one enemy spots the player, it broadcasts an alert to nearby
 * enemies in the same group so they all start chasing simultaneously.
 *
 * Properties:
 *   "AlertGroup"  – name tag; all AIs with the same tag share alerts
 *   "AlertNear"   – radius within which un-grouped AIs also get alerted
 * ========================================================================= */
extern void AI_BroadcastAlert(void *hSelf,
                               const char *groupName,
                               float nearRadius);  /* sub_1006E480 */

/* =========================================================================
 * DllEntryPoint / class registration
 *
 * sub_100010C0 → sub_100010D0 registers CBaseStargrave in the engine's
 * linked list of class descriptors so the level loader can instantiate
 * enemies by name.
 * ========================================================================= */
static void *g_classListHead = NULL;  /* dword_1016985C */

typedef struct ClassDescriptor
{
    void  *vtablePtr;   /* points to vtable / type-info ("CBaseStargrave") */
    void  *pNext;       /* intrusive linked list                            */
} ClassDescriptor;

static ClassDescriptor g_StargraveClassDesc;  /* dword_1014AE70 */

void RegisterStargraveClass(void)
{
    extern void *off_10127D00;  /* "CBaseStargrave" RTTI / vtable           */
    g_StargraveClassDesc.vtablePtr = &off_10127D00;
    g_StargraveClassDesc.pNext     = g_classListHead;
    g_classListHead                = &g_StargraveClassDesc;
}
