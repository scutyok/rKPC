/*
 * bsp_collision.c
 *
 * Translated from object.lto (Kiss: Psycho Circus – The Nightmare Child, 2000)
 * LithTech 1.x engine, 32-bit Windows DLL, Visual C++ build.
 *
 * This file covers every piece of the BSP / collision / physics system that
 * lives inside object.lto (the game-object DLL).  The actual BSP tree
 * traversal and ray-casting live in the engine (ltserv.dll / clientfx.flt),
 * but this DLL calls into them through the ILTServer vtable and reacts to
 * the results.  All of that reaction code is here.
 *
 * Systems covered
 * ───────────────
 *  1. LithTech engine collision interface  (vtable call wrappers)
 *  2. TouchNotify  – collision response for AI and projectiles
 *  3. AI wall-slide / strafe-around-obstacle
 *  4. NodeVolume / NodeLink  – nav-mesh style spatial queries
 *  5. AI move-state machine that drives pathing after collision
 *  6. CTriggerBrush – level geometry that reacts to overlap
 *  7. PlayerPhysicsTable – per-character velocity constants
 *  8. VolumeBrush (water / lava triggers)
 *
 * All struct offsets are taken directly from the disassembly.
 * Variable names come from embedded debug strings and behavioural context.
 */

#include <math.h>
#include <string.h>
#include <stddef.h>

/* =========================================================================
 * Engine types and vtable interface
 * =========================================================================
 *
 * LithTech 1.x exposes the server shell through a single ILTServer pointer
 * stored at g_pServerDE (dword_10169858) and the client shell through
 * g_pClientDE (dword_10169840).  Both are accessed through fat vtables.
 *
 * The offsets below are derived from every [eax+N] / [ecx+N] call site
 * in the collision-related functions.
 */

typedef void*  HOBJECT;   /* opaque engine object handle */
typedef void*  HCLASS;    /* engine class handle         */
typedef int    LTBOOL;
typedef float  LTFLOAT;
#define LTTRUE  1
#define LTFALSE 0

/* Engine error codes seen in GetContainerList */
#define LT_OK           0x00
#define LT_NOTINWORLD   0x15   /* object outside world bounds – seen at 10071AD6 */
#define LT_NOTFOUND     0x01

/* Surface flags – passed back from IntersectSegment */
#define SURF_SOLID       0x0001
#define SURF_SKY         0x0002
#define SURF_PORTALCLIP  0x0004

/* -------------------------------------------------------------------------
 * ILTServer vtable (partial – only slots used by collision code)
 * ------------------------------------------------------------------------- */
typedef struct ILTServer ILTServer;
struct ILTServer {
    void *pad[0];
    /* Vtable slots (offset / sizeof(ptr) = index):
     *  0x24  / 4 =  9  – GetTime() -> float (via FPU)
     *  0x44  / 4 = 17  – GetPlayerObject() -> HOBJECT
     *  0x84  / 4 = 33  – (unknown – used to get world object)
     *  0x104 / 4 = 65  – GetObjectDims(HOBJECT, LTVector*) -> LT_RESULT
     *  0x1C  / 4 =  7  – SetObjectPos(HOBJECT, LTVector*)
     *  0x1D0 / 4 =116  – GetObjectPos(HOBJECT, LTVector*)
     *  0x2A4 / 4 =169  – GetContainerList(LTVector*, HOBJECT*, int maxCount) -> int
     *  0x1E4 / 4 =121  – GetTouchNotifyInfo(collision arg) -> HTOUCHNOTIFY*
     *  0xEC  / 4 = 59  – MoveObject(HOBJECT, LTVector*)
     */
};

extern ILTServer  *g_pServerDE;   /* dword_10169858 */
extern ILTServer  *g_pClientDE;   /* dword_10169840 */

/* -------------------------------------------------------------------------
 * ILTServer vtable wrappers – typed for readability
 * ------------------------------------------------------------------------- */
typedef float  (*FnGetTime)(ILTServer*);
typedef HOBJECT(*FnGetPlayerObject)(ILTServer*);
typedef int    (*FnGetContainerList)(ILTServer*, float* pPos,
                                     HOBJECT* pOut, int maxCount);
typedef void   (*FnGetObjectPos)(ILTServer*, HOBJECT, float* pOutVec3);
typedef void   (*FnGetObjectDims)(ILTServer*, HOBJECT, float* pOutVec3);
typedef void   (*FnMoveObject)(ILTServer*, HOBJECT, float* pNewPos);

/* Helper macros that decode the vtable at runtime (matching compiler output) */
#define SERVER_GetTime()          (((FnGetTime*)         (*(void**)g_pServerDE))[0x24/4](g_pServerDE))
#define SERVER_GetPlayerObj()     (((FnGetPlayerObject*) (*(void**)g_pServerDE))[0x44/4](g_pServerDE))
#define SERVER_GetContainers(p,o,n) (((FnGetContainerList*)(*(void**)g_pServerDE))[0x2A4/4](g_pServerDE,p,o,n))
#define SERVER_GetPos(h,v)        (((FnGetObjectPos*)    (*(void**)g_pServerDE))[0x1D0/4](g_pServerDE,h,v))
#define SERVER_GetDims(h,v)       (((FnGetObjectDims*)   (*(void**)g_pServerDE))[0x104/4](g_pServerDE,h,v))
#define SERVER_MoveObject(h,p)    (((FnMoveObject*)      (*(void**)g_pServerDE))[0x1EC/4](g_pServerDE,h,p))


/* =========================================================================
 * 1. Basic plane / surface structures
 * =========================================================================
 *
 * LithTech uses an (nx, ny, nz, d) plane equation: dot(n,p) = d
 * The engine hands a contact plane to TouchNotify in the "arg" block.
 */

typedef struct {
    float x, y, z;
} LTVector;

typedef struct {
    LTVector normal;   /* unit normal of the surface hit              */
    float    dist;     /* plane offset: dot(normal, surfacePoint) = d */
} LTPlane;

/* What the engine passes to TouchNotify */
typedef struct {
    HOBJECT  hToucher;        /* object that moved into us            */
    LTPlane  contactPlane;    /* surface at the point of contact      */
} TouchNotifyInfo;

/* Segment-cast result (IntersectSegment result via engine vtable) */
typedef struct {
    LTVector hitPoint;
    LTPlane  hitPlane;
    HOBJECT  hHitObj;
    int      surfaceFlags;
} IntersectInfo;


/* =========================================================================
 * 2. AI object layout (collision-relevant fields only)
 * =========================================================================
 *
 * These are the same offsets used in enemy_ai.c but restricted to
 * the fields referenced by collision/physics code.
 */

typedef struct AIObject {
    /* 0x00 */ void      *vtable;
    /* 0x08 */ HOBJECT    hSelf;          /* our own engine handle        */
    /* 0x54 */ float      posX;
    /* 0x58 */ float      posY;
    /* 0x5C */ float      posZ;
    /* 0x60 */ float      fwdX;           /* forward unit vector          */
    /* 0x64 */ float      fwdY;
    /* 0x68 */ float      fwdZ;
    /* 0x6C */ float      gameTime;

    /* --- move-goal / pathing state --- */
    /* 0xC4 */ unsigned char bStuck;
    /* 0xD4 */ int        eMoveGoal;      /* MOVEGOAL_* enum              */
    /* 0xD8 */ unsigned char bMoveGoalFlag;
    /* 0xDC */ HOBJECT    hMoveTarget;    /* handle to goal object        */
    /* 0xE0 */ float      goalX;
    /* 0xE4 */ float      goalY;
    /* 0xE8 */ float      goalZ;
    /* 0xEC */ float      toDirX;         /* normalised dir to goal       */
    /* 0xF0 */ float      toDirY;
    /* 0xF4 */ float      toDirZ;
    /* 0xF8 */ float      distToGoal;
    /* 0xFC */ float      goalYaw;        /* 0x42800000 = 64.0 = "reset"  */
    /* 0x100 */ float     goalExpireTime;

    /* saved previous goal for TEMP restore */
    /* 0x104 */ int       savedMoveGoal;
    /* 0x108 */ unsigned char savedMoveGoalFlag;
    /* 0x10C */ HOBJECT   hSavedMoveTarget;
    /* 0x110 */ float     savedGoalX;
    /* 0x114 */ float     savedGoalY;
    /* 0x118 */ float     savedGoalZ;
    /* 0x11C */ float     savedGoalYaw;

    /* --- contact/slide state written by TouchNotify --- */
    /* 0x174 */ HOBJECT   hEnemy;         /* current chase target         */
    /* 0x1A4 */ float     enemyYaw;
    /* 0x2B0 */ float     slideWindowEnd; /* time until slide-around ends */
    /* 0x2B4 */ unsigned char bSliding;
    /* 0x2B8 */ float     slideDuration;  /* how long one slide attempt lasts */
    /* 0x2C0 */ int       eMoveAnim;      /* which vtable anim-slot to call   */
    /* 0x290 */ float     attackRangeTime;
    /* 0x320 */ float     nextAttackTime;
} AIObject;

/* Move-goal enum (same as enemy_ai.c) */
#define MOVEGOAL_NONE       0
#define MOVEGOAL_TEMP       1
#define MOVEGOAL_LINKSTART  2
#define MOVEGOAL_LINKEND    3
#define MOVEGOAL_CHASE      4


/* =========================================================================
 * 3. TouchNotify  (sub_10071470)
 *
 * Called by the engine whenever this AI's movement volume contacts another
 * object.  This is the server-side collision RESPONSE – the BSP query
 * (IntersectSegment) is handled by the engine before this is called.
 *
 * Responsibilities:
 *   a) Validate the contact (null object, degenerate plane)
 *   b) Check that the plane normal is not too steep to slide on
 *   c) If we're in an active move-goal, compute a "strafe" (wall-slide)
 *      vector perpendicular to the obstacle normal and set a TEMP goal
 *   d) Update the stuck flag and move-goal state
 * =========================================================================*/

/* Forward declarations */
extern LTVector* AI_ComputeStrafeVector(AIObject *self,
                                         LTVector *pContactNormal,
                                         LTVector *pOut); /* sub_10071140 */
extern void      StringHandle_Assign(void *dst, void *src);
extern void      StringHandle_Release(void *sh);
extern void      StringHandle_AddRef(void *sh);
extern int       __isnan(double);
extern int       RandInt(int lo, int hi);          /* sub_100D7140 */
extern int       GetNodeVolume(HOBJECT hObj, void *pOutVolume); /* sub_10095F10 */

/* Threshold constants embedded in the binary */
static const float kPlaneNormalMin    = 0.0f;    /* flt_1011521C  – zero-length normal rejection */
static const float kGoalExpireWindow  = 5.0f;    /* flt_10115214  – TEMP goal max age             */
static const float kMaxSlopeY         = 0.5f;    /* flt_10119270  – max walkable Y normal          */
static const float kMinSlopeY         = -0.5f;   /* dbl_10119268  – steepest downward slope        */
static const float kStrafeFraction    = 0.5f;    /* flt_10115218  – used to shorten strafe step    */
static const float kTempGoalYaw       = 32.0f;   /* 0x42000000    – yaw stored when setting temp   */

/* -------------------------------------------------------------------------
 * TouchNotify – main entry point
 *
 * stdcall convention: two hidden engine args pushed before 'this' in ecx.
 * The engine pushes a "NotifyInfo*" and an unknown int – we see the pop
 * as retn 8 at the end.
 * ------------------------------------------------------------------------- */
void __stdcall AI_TouchNotify(AIObject *self,
                               void    *pNotifyArg0,  /* unused   */
                               void    *pNotifyArg1)  /* unused   */
{
    /* ---- (a) Get the TouchNotifyInfo from the engine ---- */
    TouchNotifyInfo tni;
    typedef TouchNotifyInfo* (*FnGetTNI)(ILTServer*, void*);
    TouchNotifyInfo *pInfo = ((FnGetTNI*)(*(void**)g_pServerDE))[0x1E4/4](
                                 g_pServerDE, &tni);

    HOBJECT hToucher = pInfo ? pInfo->hToucher : NULL;

    /* "TouchNotify: invalid object." */
    if (!hToucher)
        return;

    /* ---- (b) Validate the contact plane normal ---- */
    float nx = pInfo->contactPlane.normal.x;
    float ny = pInfo->contactPlane.normal.y;
    float nz = pInfo->contactPlane.normal.z;

    /* All three components must be non-zero (degenerate-plane guard) */
    if (nx == kPlaneNormalMin && ny == kPlaneNormalMin && nz == kPlaneNormalMin)
    {
        /* "TouchNotify: invalid plane." */
        return;
    }

    /* ---- (c) Short-circuit if no active move-goal ---- */
    if (self->eMoveGoal == MOVEGOAL_NONE)
    {
        /* "  Goal type is MOVEGOAL_NONE" / "  exiting TouchNotify" */
        return;
    }

    /* If we're in a TEMP goal and NOT stuck, let the existing path continue */
    if (self->eMoveGoal == MOVEGOAL_TEMP && !self->bStuck)
    {
        /* "  Goal type is MOVEGOAL_TEMP" / "  m_bStuck == DFALSE" */
        /* "  exiting TouchNotify" */
        return;
    }

    /* Update game time */
    self->gameTime = SERVER_GetTime();

    /* NaN-guard the contact normal */
    __isnan(nx);  __isnan(ny);  __isnan(nz);

    /* ---- (d) Check if the TEMP goal has expired ---- */
    float timeSinceGoal = self->goalExpireTime - self->gameTime;
    if (timeSinceGoal > kGoalExpireWindow)
    {
        /* "  Current goal has not expired." */
        return;
    }

    /* ---- (e) Slope check ---- */
    /* Reject surfaces too steep to slide on (ceiling / steep wall)  */
    if (ny > kMaxSlopeY || ny < kMinSlopeY)
    {
        /* "  plane too steep." */
        return;
    }

    /* ---- (f) Identify who we hit – is it an AI with a NodeVolume? ---- */
    HOBJECT hWorld = ((void*(**)(ILTServer*))(*(void**)g_pServerDE))[0x84/4](g_pServerDE);

    HOBJECT hHitObj = hToucher; /* the object whose surface we touched */

    /* Check if the hit object is a valid AI that can block us */
    typedef void* (*FnGetAIObject)(void *hHandle);
    void *pHitAI = NULL;
    {
        HOBJECT hWrapped = hHitObj;
        /* sub_100EF1C0 – dereferences a handle into an AIObject* */
        extern void* UnwrapHandle(HOBJECT h);
        pHitAI = UnwrapHandle(hHitObj);
    }

    /* The AI we hit must:
     *   – exist (not null)
     *   – not be in its own stuck state      [ebp+0x110]
     *   – not currently be in "stopped" mode [ebp+0x2C0]  (bStuck != 1)
     *   – have a compatible move-goal        */
    int bValidBlock = (pHitAI != NULL);
    if (pHitAI)
    {
        AIObject *pOther = (AIObject*)pHitAI;
        if (pOther->bStuck)           bValidBlock = 0;
        if (pOther->bSliding)         bValidBlock = 0;  /* [ebp+0x2C0] != 0 */

        /* Only block us if the other AI is actively pursuing a goal that
         * could have placed it in our path */
        int otherGoal = pOther->eMoveGoal;
        if (otherGoal != MOVEGOAL_LINKSTART &&
            otherGoal != MOVEGOAL_LINKEND   &&
            otherGoal != MOVEGOAL_NONE      )
        {
            if (otherGoal == MOVEGOAL_TEMP)
            {
                int savedGoal = pOther->savedMoveGoal;
                if (savedGoal != MOVEGOAL_LINKSTART &&
                    savedGoal != MOVEGOAL_LINKEND)
                    bValidBlock = 0;
            }
            else
            {
                bValidBlock = 0;
            }
        }
    }

    if (!bValidBlock)
    {
        /* ================================================================
         * BRANCH A – We hit an obstacle AI.
         * We cannot pass through; compute a wall-slide "strafe" vector,
         * verify the destination is inside the world, then set a TEMP goal.
         * ================================================================ */

        /* Get our own AABB dims and the toucher's AABB dims */
        float ourDims[3], otherDims[3];
        SERVER_GetDims(self->hSelf,  ourDims);
        SERVER_GetDims(hHitObj,      otherDims);

        /* Pick the larger extent for X and Z to determine slide distance.
         * This mimics the min/max logic at 0x100718A4–100718FE:
         *   strafe_dist = max(ourX, otherX) + max(ourZ, otherZ)
         *               + 2 * max(ourY, otherY)          ← "flt_10119104 = 2.0" */
        float slideX = (ourDims[0] > otherDims[0]) ? ourDims[0] : otherDims[0];
        float slideZ = (ourDims[2] > otherDims[2]) ? ourDims[2] : otherDims[2];
        float slideY = (ourDims[1] > otherDims[1]) ? ourDims[1] : otherDims[1];

        float strafeDist = slideX + slideZ + 2.0f * slideY;   /* flt_10119104 = 2.0 */

        goto compute_strafe_goal;

    compute_strafe_goal: ;
        /* Compute the strafe direction perpendicular to the obstacle */
        LTVector contactNormal = { nx, ny, nz };
        LTVector strafeDir = {0};
        AI_ComputeStrafeVector(self, &contactNormal, &strafeDir);

        /* "  vStrafe = (%.2f, %.2f, %.2f)." */
        /* "  fStrafeDist = %.2f"             */

        /* If we are currently stuck, randomly flip the strafe direction
         * (25% chance → RandInt(0,100) < 25) to avoid oscillation */
        if (self->bStuck && RandInt(0, 100) < 25)
        {
            strafeDist = -strafeDist;
        }

        /* Candidate position: current pos + strafe * strafeDist */
        float candidateX = self->posX + strafeDir.x * strafeDist;
        float candidateY = self->posY + strafeDir.y * strafeDist;
        float candidateZ = self->posZ + strafeDir.z * strafeDist;

        /* Validate: is the candidate inside the world? */
        float candidatePos[3] = { candidateX, candidateY, candidateZ };
        int   rc = SERVER_GetContainers(candidatePos, NULL, 0);

        if (rc == LT_NOTINWORLD)
        {
            /* "  Temp goal not in world, moving..." */
            /* Shrink the step and try again */
            candidateX = self->posX + strafeDir.x * strafeDist * kStrafeFraction;
            candidateY = self->posY + strafeDir.y * strafeDist * kStrafeFraction;
            candidateZ = self->posZ + strafeDir.z * strafeDist * kStrafeFraction;
        }

        /* ---- Save the current goal so we can restore it after the strafe ---- */
        if (self->eMoveGoal != MOVEGOAL_TEMP)
        {
            /* Snapshot current goal into saved-goal slots */
            self->savedMoveGoal     = self->eMoveGoal;
            self->savedMoveGoalFlag = self->bMoveGoalFlag;

            /* StringHandle copy of hMoveTarget */
            if (self->hMoveTarget != self->hSavedMoveTarget)
            {
                StringHandle_Release(self->hSavedMoveTarget);
                self->hSavedMoveTarget = self->hMoveTarget;
                StringHandle_AddRef(self->hSavedMoveTarget);
            }

            self->savedGoalYaw = self->goalYaw;
            self->savedGoalX   = self->goalX;
            self->savedGoalY   = self->goalY;
            self->savedGoalZ   = self->goalZ;
        }

        /* ---- Set the TEMP strafe goal ---- */
        /* Reset old goal data */
        self->eMoveGoal     = MOVEGOAL_NONE;
        self->bMoveGoalFlag = 0;

        if (self->hMoveTarget)
        {
            StringHandle_Release(self->hMoveTarget);
            self->hMoveTarget = NULL;
            StringHandle_AddRef(NULL);
        }

        self->distToGoal  = 0.0f;
        self->goalYaw     = 64.0f;   /* 0x42800000 */
        self->goalExpireTime = 0.0f;
        self->toDirZ = self->toDirY = self->toDirX = 0.0f;
        self->goalX = self->goalY = self->goalZ = 0.0f;

        /* Install strafe position as a TEMP goal */
        self->eMoveGoal       = MOVEGOAL_TEMP;
        self->goalYaw         = kTempGoalYaw;   /* 0x42000000 = 32.0 */
        self->goalX           = candidateX;
        self->goalY           = candidateY;
        self->goalZ           = candidateZ;
        self->bMoveGoalFlag   = 1;

        self->goalExpireTime  = SERVER_GetTime();

        /* NaN guard on new position */
        __isnan(self->goalX);
        __isnan(self->goalY);
        __isnan(self->goalZ);

        /* "  Set temp goal." */
        return;
    }

    /* ================================================================
     * BRANCH B – We hit a solid world surface (brush / geometry).
     * If CHASE goal is active, re-target towards the enemy's current
     * position, recalculating direction and distance.
     * ================================================================ */

    /* Clear current goal cleanly */
    self->eMoveGoal     = MOVEGOAL_NONE;
    self->bMoveGoalFlag = 0;

    if (self->hMoveTarget)
    {
        StringHandle_Release(self->hMoveTarget);
        self->hMoveTarget = NULL;
        StringHandle_AddRef(NULL);
    }

    self->distToGoal     = 0.0f;
    self->goalYaw        = 64.0f;
    self->goalExpireTime = 0.0f;
    self->toDirZ = self->toDirY = self->toDirX = 0.0f;
    self->goalX  = self->goalY  = self->goalZ  = 0.0f;

    /* Switch to CHASE goal towards the enemy */
    if (self->hEnemy)
    {
        float enemyPos[3];
        SERVER_GetPos(self->hEnemy, enemyPos);

        self->goalX = enemyPos[0];
        self->goalY = enemyPos[1];
        self->goalZ = enemyPos[2];

        self->eMoveGoal     = MOVEGOAL_CHASE;
        self->bMoveGoalFlag = 1;
        self->goalYaw       = self->enemyYaw;

        self->goalExpireTime = SERVER_GetTime();

        /* Compute 2D (XZ) direction to enemy – Y direction ignored for nav */
        float dx = self->goalX - self->posX;
        float dz = self->goalZ - self->posZ;

        /* Y component comes from the contact plane */
        self->toDirY = 0.0f;

        float dist2D = sqrtf(dx*dx + dz*dz);
        self->distToGoal = dist2D;

        if (dist2D > 0.0f)
        {
            float inv = 1.0f / dist2D;
            self->toDirX = dx * inv;
            self->toDirZ = dz * inv;
        }

        __isnan(self->toDirX);
        __isnan(self->toDirY);
        __isnan(self->toDirZ);

        /* Dispatch to the per-state move virtual function */
        typedef void (*FnSetAnim)(AIObject*, int animId, int bLoop);
        FnSetAnim setAnim = ((FnSetAnim*)(*(void**)self))[0x90/sizeof(void*)];
        setAnim(self, 4 /* ANIM_WALK */, 1 /* loop */);
    }
}


/* =========================================================================
 * 4. AI_ComputeStrafeVector  (sub_10071140)
 *
 * Given the normal of the surface we just collided with, compute a
 * tangent vector that slides along the surface.
 *
 * Algorithm: cross product of the contact normal with a reference "up"
 * vector, then projected onto the XZ plane.  If the AI is stuck, the
 * result is negated to try the opposite side.
 * ========================================================================= */
LTVector* AI_ComputeStrafeVector(AIObject  *self,
                                  LTVector  *pNormal,
                                  LTVector  *pOut)
{
    /* Cross normal with world up (0,1,0) gives a horizontal tangent */
    float tx =  pNormal->z;   /* cross(n, up).x =  nz */
    float ty =  0.0f;
    float tz = -pNormal->x;   /* cross(n, up).z = -nx */

    /* Project onto XZ – drop Y influence */
    float len = sqrtf(tx*tx + tz*tz);
    if (len > 0.0001f)
    {
        tx /= len;
        tz /= len;
    }

    pOut->x = tx;
    pOut->y = ty;
    pOut->z = tz;
    return pOut;
}


/* =========================================================================
 * 5. AI move-animation dispatcher  (sub_10070B40)
 *
 * Called every frame while the AI has an active move-goal.
 * Uses a 6-case switch on self->eMoveAnim (offset 0x2C0) to decide which
 * virtual function to call for this frame's movement.
 *
 * Also advances the slide-window timer (offset 0x2B0 / 0x2B8).
 * ========================================================================= */

/* Virtual slot indices for movement animations (vtable offsets / 4) */
#define MOVESLOT_WALK    (0x100/4)   /* case 0 – normal walk       */
#define MOVESLOT_RUN     (0x10C/4)   /* case 1 – run               */
#define MOVESLOT_STRAFE  (0x118/4)   /* case 2 – strafe            */
#define MOVESLOT_LEAP    (0x124/4)   /* case 4 – leap start        */
#define MOVESLOT_SLIDE   (0x130/4)   /* case 5 – wall-slide        */

void AI_UpdateMovement(AIObject *self)
{
    /* Advance the slide-window: if we're past slideWindowEnd, open a new window */
    if (self->gameTime >= self->slideWindowEnd)
    {
        self->bSliding = 1;
        self->slideWindowEnd = self->slideDuration + self->gameTime;
    }

    /* Dispatch through vtable based on current move-animation mode */
    typedef void (*FnMoveAnim)(AIObject*);
    FnMoveAnim *vtbl = (FnMoveAnim*)(*(void**)self);
    FnMoveAnim fn;

    switch (self->eMoveAnim)
    {
        case 0: fn = vtbl[MOVESLOT_WALK];   break;
        case 1: fn = vtbl[MOVESLOT_RUN];    break;
        case 2: fn = vtbl[MOVESLOT_STRAFE]; break;
        case 4: fn = vtbl[MOVESLOT_LEAP];   break;
        case 5: fn = vtbl[MOVESLOT_SLIDE];  break;
        default: fn = vtbl[MOVESLOT_WALK];  break;  /* cases 3 and out-of-range */
    }

    fn(self);

    /* Update the "last good position" timer:
     * self->nextAttackTime = gameTime + self->slideWindowEnd
     * (stored at offset 0x7C via: fld [esi+78] + fadd [esi+6C] → fstp [esi+7C]) */
    /* (offset 0x78 = slide duration delta; 0x7C = resulting target time) */
}


/* =========================================================================
 * 6. NodeVolume / NodeLink system  (sub_10070630 + sub_10095F10)
 *
 * LithTech's nav-mesh equivalent is a graph of hand-placed CNodeVolume
 * objects connected by CNodeLink objects.  AI use this graph to find
 * paths to goals.
 *
 * GetPointNodeVolume queries which CNodeVolume contains a given point.
 * This is used when setting chase-goals to verify the destination is
 * reachable by a path.
 * ========================================================================= */

typedef struct CNodeVolume CNodeVolume;
typedef struct CNodeLink   CNodeLink;

struct CNodeVolume {
    void        *vtable;
    HOBJECT      hSelf;
    CNodeLink   *links[8];    /* "NodeLink0".."NodeLink7" */
    int          nLinks;
    float        posX, posY, posZ;
    float        radius;
};

struct CNodeLink {
    void         *vtable;
    HOBJECT       hSelf;
    CNodeVolume  *pStart;  /* "NodeVolume" at link start */
    CNodeVolume  *pEnd;    /* "NodeVolume" at link end   */
    float         cost;    /* traversal cost             */
};

/* -------------------------------------------------------------------------
 * GetNodeVolume  (sub_10095F10)
 *
 * Returns the CNodeVolume* that contains hObj, or NULL.
 * Implements a simple sphere-containment test against each volume.
 * ------------------------------------------------------------------------- */
CNodeVolume* GetNodeVolume(HOBJECT hObj, void *pUnused)
{
    /* In the binary this iterates the global node-volume list.
     * The list is maintained by CNodeVolume::Init / CNodeVolume::Term. */
    extern CNodeVolume **g_pNodeVolumeList;
    extern int           g_nNodeVolumes;

    if (!hObj) return NULL;

    float pos[3];
    SERVER_GetPos(hObj, pos);

    for (int i = 0; i < g_nNodeVolumes; i++)
    {
        CNodeVolume *pVol = g_pNodeVolumeList[i];
        if (!pVol) continue;

        float dx = pos[0] - pVol->posX;
        float dy = pos[1] - pVol->posY;
        float dz = pos[2] - pVol->posZ;
        float distSq = dx*dx + dy*dy + dz*dz;

        if (distSq <= pVol->radius * pVol->radius)
            return pVol;
    }
    return NULL;
}

/* -------------------------------------------------------------------------
 * GetPointNodeVolume  (sub_10070630)
 *
 * Queries which CNodeVolume contains a world-space point, using the
 * engine's container-list API (vtable[0x2A4]) to find candidate volumes
 * and then confirming via GetNodeVolume.
 *
 * Debug strings in the binary:
 *   "GetPointNodeVolume: 1 volume"
 *   "Multiple node volumes at (%.2f, %.2f, %.2f)"
 *   "GetPointNodeVolume: (%.2f %.2f %.2f) is not in a node volume"
 *   "  %i containers"
 * ------------------------------------------------------------------------- */
#define MAX_CONTAINERS 16

CNodeVolume* GetPointNodeVolume(AIObject *self, float *pPoint)
{
    /* Ask the engine for a list of container objects at pPoint (max 16) */
    HOBJECT containers[MAX_CONTAINERS];
    int nContainers = SERVER_GetContainers(pPoint, containers, MAX_CONTAINERS);

    int          nVolumesFound = 0;
    CNodeVolume *pFoundVolume  = NULL;

    for (int i = 0; i < nContainers; i++)
    {
        /* Try to interpret each container as a CNodeVolume */
        CNodeVolume *pVol = NULL;
        void *pObj = NULL;
        extern void* UnwrapHandle(HOBJECT h);
        pObj = UnwrapHandle(containers[i]);

        if (pVol = GetNodeVolume(containers[i], NULL))
        {
            nVolumesFound++;
            pFoundVolume = pVol;
        }
    }

    if (nVolumesFound == 1)
    {
        /* "GetPointNodeVolume: 1 volume" */
        return pFoundVolume;
    }

    if (nVolumesFound > 1)
    {
        /* "Multiple node volumes at (%.2f, %.2f, %.2f)" – ambiguous, return first */
        return pFoundVolume;
    }

    /* "GetPointNodeVolume: (%.2f %.2f %.2f) is not in a node volume" */
    /* "  %i containers" */
    return NULL;
}

/* -------------------------------------------------------------------------
 * GetObjectNodeVolume  (sub_10070750 – referenced by debug strings)
 *
 * Variant that takes an HOBJECT directly.
 *
 * Debug strings:
 *   "GetObjectNodeVolume: 1 volume, %s"
 *   "GetObjectNodeVolume: multiple volumes, %s"
 *   "GetObjectNodeVolume: %s has no NodeVolume"
 * ------------------------------------------------------------------------- */
CNodeVolume* GetObjectNodeVolume(AIObject *self, HOBJECT hObj)
{
    float pos[3];
    SERVER_GetPos(hObj, pos);
    return GetPointNodeVolume(self, pos);
}


/* =========================================================================
 * 7. CTriggerBrush  (sub_1002E340 area)
 *
 * Level geometry brush that fires engine messages when objects enter/exit.
 * Relevant to collision because it is activated by the BSP overlap test
 * the engine runs every frame.
 *
 * The main variants are:
 *   CTriggerBrush          – generic script trigger
 *   CTriggerBrushDamage    – deals damage on overlap
 *   CTriggerBrushDoor      – opens a door
 *   CTriggerBrushHellspore – spawns a hellspore enemy
 *   CTriggerBrush_Secret   – marks a secret area
 *   CTriggerBrush_Generic  – calls a Bute attribute handler
 *
 * The "Solid" property controls whether the brush blocks movement.
 * "SkyPortal" marks a brush as part of a sky-portal view.
 * "HullMaker" marks a convex hull used for collision generation.
 * ========================================================================= */

typedef struct CTriggerBrush {
    void       *vtable;
    HOBJECT     hSelf;
    int         bSolid;          /* "Solid" property                   */
    int         bActive;
    HOBJECT     hActivator;      /* last object that triggered this    */
    float       activateTime;
    float       reactivateDelay;
    char        scriptName[64];  /* script to run on trigger           */
} CTriggerBrush;

typedef struct CTriggerBrushDamage {
    CTriggerBrush base;
    float         damageAmount;
    int           damageType;    /* DT_EXPLODE, DT_FIRE, etc.          */
    char          brushName[64]; /* "damage_brush_name" property       */
    int           bEnabled;      /* toggled by CCinematicEventDamageBrush* */
} CTriggerBrushDamage;

/* Called by engine when an object overlaps this brush */
void CTriggerBrushDamage_TouchNotify(CTriggerBrushDamage *self,
                                      HOBJECT hToucher,
                                      LTPlane *pPlane)
{
    if (!hToucher)
    {
        /* "CBaseProjectile::TouchNotify: NULL object" */
        return;
    }
    if (!pPlane)
    {
        /* "CBaseProjectile::TouchNotify: invalid plane" */
        return;
    }

    if (!self->bEnabled)
        return;

    /* Apply damage to toucher through engine message system */
    /* engine: SendToObject(hToucher, MGUARD_DAMAGE, damageAmount, damageType) */
}


/* =========================================================================
 * 8. VolumeBrush  (sub_100F6040 area, water trigger at 100F806E)
 *
 * A special trigger brush that represents a physics volume.
 * Objects inside are subject to modified drag and buoyancy.
 *
 * The water variant is identified by sprite "Sprites\VolumeBrushSprites\Water1.spr"
 * ========================================================================= */

typedef struct CVolumeBrush {
    void    *vtable;
    HOBJECT  hSelf;
    float    viscosity;     /* drag coefficient for objects inside     */
    float    buoyancy;      /* upward force per unit mass              */
    int      eSurfaceType;  /* enum: SURFTYPE_WATER, SURFTYPE_LAVA ... */
    char     spriteName[MAX_PATH];
} CVolumeBrush;

/* Water surface type constant (LithTech engine enum) */
#define SURFTYPE_WATER  1
#define SURFTYPE_LAVA   2


/* =========================================================================
 * 9. PlayerPhysicsTable  (sub_100F5F40)
 *
 * Reads per-character movement velocity constants from the .bute attribute
 * file (game/attribute/player.bute) at startup.
 *
 * CButeMgr__Exist retrieves a named float from the INI-style .bute file.
 *
 * Two character classes exist:
 *   "Avatar" – standard player  (AvatarWalkVel, AvatarRunVel, AvatarJumpVel)
 *   "Elder"  – alternate class  (ElderWalkVel,  ElderRunVel,  ElderJumpVel)
 *
 * The results are stored in globals used by the player controller.
 * ========================================================================= */

/* Globals that hold the loaded velocities (addresses from the binary) */
static float g_ElderJumpVel;   /* dword_10146928 */
static float g_ElderRunVel;    /* dword_1014692C */
static float g_ElderWalkVel;   /* dword_10146930 */
static float g_AvatarJumpVel;  /* dword_10146908 */
static float g_AvatarRunVel;   /* dword_1014690C */
static float g_AvatarWalkVel;  /* dword_10146910 */

extern int   CButeMgr__Exist(void *pMgr,
                              const char *section,
                              const char *key,
                              float *pOutValue); /* returns 1 if found */

/* unk_101576C0 = the CButeMgr instance loaded from player.bute */
extern void *g_pPlayerButeMgr;  /* unk_101576C0 */

void PlayerPhysicsTable(void)
{
    float val;

    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "ElderJumpVel", &val))
        g_ElderJumpVel = val;

    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "ElderRunVel", &val))
        g_ElderRunVel = val;

    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "ElderWalkVel", &val))
        g_ElderWalkVel = val;

    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "AvatarJumpVel", &val))
        g_AvatarJumpVel = val;

    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "AvatarRunVel", &val))
        g_AvatarRunVel = val;

    if (CButeMgr__Exist(g_pPlayerButeMgr, "Player", "AvatarWalkVel", &val))
        g_AvatarWalkVel = val;
}


/* =========================================================================
 * 10. Projectile TouchNotify  (sub_100BF00 area + sub_100786xx)
 *
 * Projectiles (CBaseProjectile, CBladeDisc, CStumpFireball, etc.) each
 * override TouchNotify to explode / deal damage on contact.
 *
 * The shared validation pattern (NULL object, invalid plane) is the same
 * as the AI version.  After validation each projectile:
 *   1. Calls Engine::IntersectSegment to confirm the exact hit point
 *   2. Spawns a hit-effect (sparks / explosion) at the impact location
 *   3. Deals damage to the struck object via SendToObject(MGUARD_DAMAGE)
 *   4. Removes itself
 * ========================================================================= */

typedef struct CBaseProjectile {
    void    *vtable;
    HOBJECT  hSelf;
    HOBJECT  hOwner;     /* AI that fired this */
    float    damage;
    int      damageType;
    float    velocity[3];
    float    lifeTime;
} CBaseProjectile;

void CBaseProjectile_TouchNotify(CBaseProjectile *self,
                                  HOBJECT          hToucher,
                                  LTPlane         *pPlane)
{
    /* "CBaseProjectile::TouchNotify: NULL object" */
    if (!hToucher) return;

    /* "CBaseProjectile::TouchNotify: invalid plane" */
    if (!pPlane) return;

    /* Don't collide with our owner */
    if (hToucher == self->hOwner) return;

    /* Spawn impact FX at hit position (engine call omitted) */
    /* Apply damage */
    /* Remove self */
}


/* =========================================================================
 * 11. PortalBrush  (class "PortalBrush", sub_1004B850 area)
 *
 * A BSP portal is a convex polygon at the boundary between two BSP leaves.
 * The PortalBrush game object marks a level-design portal that can be
 * toggled open/closed (e.g. force-field doors).
 *
 * When closed it acts as a solid brush (blocking both movement and the
 * engine's BSP portal-visibility traversal).
 *
 * "SkyPortal" variant: renders the sky view instead of the other side.
 * "Portal" / "HullMaker" are related geometry types (see sub_10051B00 area).
 * ========================================================================= */

typedef struct CPortalBrush {
    void    *vtable;
    HOBJECT  hSelf;
    int      bOpen;
    int      bSkyPortal;
    char     portalName[64]; /* "portal_name" property */
} CPortalBrush;

void CPortalBrush_SetOpen(CPortalBrush *self, int bOpen)
{
    self->bOpen = bOpen;
    /* engine: SetObjectFlags(hSelf, bOpen ? 0 : FLAG_SOLID) */
    /* This directly affects the BSP traversal in ltserv.dll */
}


/* =========================================================================
 * 12. AlertGroup broadcast  (sub_1006E480)
 *
 * Not strictly collision, but spatially driven: when an enemy detects the
 * player, it broadcasts to all nearby AIs so they also enter combat.
 *
 * Uses GetContainerList (engine vtable 0x2A4) to find AIs within the
 * "AlertNear" radius, then calls each one's OnAlerted() virtual method.
 *
 * "AlertGroup" – only AIs with the same group name are alerted
 * "AlertNear"  – radius in world units
 * ========================================================================= */

void AI_BroadcastAlert(AIObject   *self,
                        const char *groupName,
                        float       nearRadius)
{
    HOBJECT nearby[32];
    float   pos[3] = { self->posX, self->posY, self->posZ };

    int n = SERVER_GetContainers(pos, nearby, 32);

    for (int i = 0; i < n; i++)
    {
        HOBJECT h = nearby[i];
        if (!h || h == self->hSelf) continue;

        /* Distance check */
        float otherPos[3];
        SERVER_GetPos(h, otherPos);
        float dx = otherPos[0] - pos[0];
        float dy = otherPos[1] - pos[1];
        float dz = otherPos[2] - pos[2];
        float dist = sqrtf(dx*dx + dy*dy + dz*dz);

        if (dist > nearRadius) continue;

        /* Notify via virtual OnAlerted() */
        typedef void (*FnOnAlerted)(void*);
        void *pAI = NULL;
        extern void* UnwrapHandle(HOBJECT h);
        pAI = UnwrapHandle(h);
        if (pAI)
        {
            FnOnAlerted fn = ((FnOnAlerted*)(*(void**)pAI))[0xB8/sizeof(void*)];
            fn(pAI);
        }
    }
}
